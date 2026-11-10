//! Interactive terminal bridging for SSH sessions.
//!
//! Connects the local terminal (stdin/stdout) to a remote SSH session
//! via the VibeShell GUI's IPC server.
//!
//! Architecture:
//! - Output: A persistent IPC connection streams `SessionOutput` messages
//!   from the GUI and writes the raw bytes to stdout.
//! - Input: Separate one-shot IPC connections send `SendInput` messages
//!   for each chunk of stdin data.
//! - Resize: Terminal resize events are forwarded via `Resize` messages.

use std::fmt;
use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{bail, Result};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::terminal;
use vibeshell_core::ipc::IpcMessage;

use crate::ipc_support;

const COMMAND_IDLE_TIMEOUT: Duration = Duration::from_millis(1200);
const COMMAND_MAX_DURATION: Duration = Duration::from_secs(20);

pub enum CommandHandoff {
    Completed(Option<i32>),
    AwaitingInput,
    SessionEnded(Option<i32>),
}

#[derive(Debug)]
pub struct RemoteCommandExit(pub i32);

impl fmt::Display for RemoteCommandExit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Remote command exited with status {}", self.0)
    }
}

impl std::error::Error for RemoteCommandExit {}

/// Shell integration emits OSC 3008 start/end events with a matching command
/// ID and an exit status. Track them across arbitrary streaming byte chunks;
/// a prompt alone cannot tell us whether the command succeeded.
#[derive(Default)]
struct ShellCommandStatus {
    escape: bool,
    in_osc: bool,
    osc: Vec<u8>,
    command_id: Option<String>,
    exit_code: Option<i32>,
}

impl ShellCommandStatus {
    fn observe(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            if self.in_osc {
                if byte == b'\x07' || (byte == b'\\' && self.osc.last() == Some(&b'\x1b')) {
                    if byte == b'\\' {
                        self.osc.pop();
                    }
                    self.finish_osc();
                } else if self.osc.len() < 2048 {
                    self.osc.push(byte);
                } else {
                    self.osc.clear();
                    self.in_osc = false;
                }
            } else if self.escape && byte == b']' {
                self.in_osc = true;
                self.osc.clear();
                self.escape = false;
            } else {
                self.escape = byte == b'\x1b';
            }
        }
    }

    fn finish_osc(&mut self) {
        self.in_osc = false;
        self.escape = false;
        if let Ok(payload) = std::str::from_utf8(&self.osc) {
            let mut parts = payload.split(';');
            if parts.next() == Some("3008") {
                let mut start = None;
                let mut end = None;
                let mut kind = None;
                let mut outcome = None;
                let mut status = None;
                for part in parts {
                    if let Some((key, value)) = part.split_once('=') {
                        match key {
                            "start" => start = Some(value),
                            "end" => end = Some(value),
                            "type" => kind = Some(value),
                            "exit" => outcome = Some(value),
                            "status" => status = value.parse::<i32>().ok(),
                            _ => {}
                        }
                    }
                }
                if kind == Some("command") {
                    if let Some(id) = start {
                        self.command_id = Some(id.to_owned());
                        self.exit_code = None;
                    }
                } else if end.is_some() && end == self.command_id.as_deref() {
                    self.exit_code = match outcome {
                        Some("success") => Some(0),
                        Some("failure") => status.filter(|code| *code != 0).or(Some(1)),
                        _ => None,
                    };
                }
            }
        }
        self.osc.clear();
    }
}

/// Run an interactive terminal session connected to the given session ID.
///
/// This enters raw terminal mode, streams SSH output to stdout, and
/// forwards stdin input to the remote session. Press `Ctrl+]` to detach
/// (the session keeps running in the background).
pub fn run_interactive(session_id: &str) -> Result<()> {
    // Open streaming attach connection
    let mut reader = ipc_support::connect_streaming(&IpcMessage::AttachSession {
        session_id: session_id.to_string(),
    })?;

    // Read the initial Ok/Error response
    let mut first_line = String::new();
    reader
        .read_line(&mut first_line)
        .map_err(|e| anyhow::anyhow!("Failed to read attach response: {}", e))?;

    let response: IpcMessage = serde_json::from_str(first_line.trim())
        .map_err(|e| anyhow::anyhow!("Failed to parse attach response: {}", e))?;

    match response {
        IpcMessage::Ok => {}
        IpcMessage::Error { message } => {
            bail!("Failed to attach: {}", message);
        }
        _ => {
            bail!("Unexpected response from GUI");
        }
    }

    // Send initial resize to match current terminal size
    if let Ok((cols, rows)) = terminal::size() {
        let _ = ipc_support::send(&IpcMessage::Resize {
            session_id: session_id.to_string(),
            cols: cols as u32,
            rows: rows as u32,
        });
    }

    // Enter raw terminal mode
    terminal::enable_raw_mode()?;

    // Run the interactive loop, ensuring raw mode is always restored
    let result = run_interactive_loop(session_id, reader);

    // Always restore terminal, even if the loop errored
    let _ = terminal::disable_raw_mode();

    result
}

/// Inner interactive loop, separated so the caller can guarantee cleanup.
fn run_interactive_loop<R: BufRead + Send + 'static>(session_id: &str, reader: R) -> Result<()> {
    let done = Arc::new(AtomicBool::new(false));
    let sid = session_id.to_string();

    // --- Output thread: read SessionOutput from the streaming connection, write to stdout ---
    let done_clone = done.clone();
    let output_thread = std::thread::spawn(move || -> Result<()> {
        let mut reader = reader;
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => {
                    // EOF — server closed connection
                    break;
                }
                Ok(_) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    match serde_json::from_str::<IpcMessage>(trimmed) {
                        Ok(IpcMessage::SessionOutput { data, .. }) => {
                            let mut stdout = std::io::stdout().lock();
                            let _ = stdout.write_all(&data);
                            let _ = stdout.flush();
                        }
                        Ok(IpcMessage::SessionEnded { reason }) => {
                            // Print reason after restoring terminal
                            done_clone.store(true, Ordering::SeqCst);
                            eprintln!("\r\nSession ended: {}", reason);
                            break;
                        }
                        Ok(_) => {}  // ignore other messages
                        Err(_) => {} // ignore parse errors
                    }
                }
                Err(_) => {
                    // Read error — connection lost
                    break;
                }
            }
        }
        done_clone.store(true, Ordering::SeqCst);
        Ok(())
    });

    // --- Input loop (main thread): read terminal events, send via one-shot IPC ---
    while !done.load(Ordering::SeqCst) {
        // Poll with a short timeout so we can check the `done` flag
        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) => {
                    // Ctrl+] = detach (like SSH escape)
                    if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char(']') {
                        break;
                    }

                    let bytes = key_event_to_bytes(&key);
                    if !bytes.is_empty() {
                        let _ = ipc_support::send(&IpcMessage::SendUserInput {
                            session_id: sid.clone(),
                            data: bytes,
                        });
                    }
                }
                Event::Resize(cols, rows) => {
                    let _ = ipc_support::send(&IpcMessage::Resize {
                        session_id: sid.clone(),
                        cols: cols as u32,
                        rows: rows as u32,
                    });
                }
                Event::Paste(text) if !text.is_empty() => {
                    let _ = ipc_support::send(&IpcMessage::SendUserInput {
                        session_id: sid.clone(),
                        data: text.into_bytes(),
                    });
                }
                _ => {}
            }
        }
    }

    // Ctrl+] leaves the shared shell running; do not block waiting for its EOF.
    if done.load(Ordering::SeqCst) {
        let _ = output_thread.join();
    }

    if !done.load(Ordering::SeqCst) {
        eprintln!(
            "Detached from session {}. It continues running in the background.",
            sid
        );
        eprintln!(
            "Use 'vibeshell attach {}' to reattach or 'vibeshell kill {}' to terminate.",
            sid, sid
        );
    }

    Ok(())
}

/// Convert a crossterm `KeyEvent` to the corresponding byte sequence
/// that a terminal would send over a PTY.
fn key_event_to_bytes(key: &KeyEvent) -> Vec<u8> {
    if key.kind == KeyEventKind::Release {
        return vec![];
    }

    match key.code {
        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                // Ctrl+A = 0x01, Ctrl+B = 0x02, ..., Ctrl+Z = 0x1A
                if c.is_ascii_lowercase() || c.is_ascii_uppercase() {
                    let ctrl = (c.to_ascii_lowercase() as u8)
                        .wrapping_sub(b'a')
                        .wrapping_add(1);
                    return vec![ctrl];
                }
                // Ctrl+[ = ESC (0x1B), Ctrl+\ = 0x1C, Ctrl+] = 0x1D
                match c {
                    '[' => return vec![0x1B],
                    '\\' => return vec![0x1C],
                    ']' => return vec![0x1D],
                    _ => {}
                }
            }
            let mut buf = [0u8; 4];
            let s = c.encode_utf8(&mut buf);
            s.as_bytes().to_vec()
        }
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Tab => vec![b'\t'],
        KeyCode::Esc => vec![0x1b],
        KeyCode::Up => b"\x1b[A".to_vec(),
        KeyCode::Down => b"\x1b[B".to_vec(),
        KeyCode::Right => b"\x1b[C".to_vec(),
        KeyCode::Left => b"\x1b[D".to_vec(),
        KeyCode::Home => b"\x1b[H".to_vec(),
        KeyCode::End => b"\x1b[F".to_vec(),
        KeyCode::PageUp => b"\x1b[5~".to_vec(),
        KeyCode::PageDown => b"\x1b[6~".to_vec(),
        KeyCode::Insert => b"\x1b[2~".to_vec(),
        KeyCode::Delete => b"\x1b[3~".to_vec(),
        KeyCode::F(n) => match n {
            1 => b"\x1bOP".to_vec(),
            2 => b"\x1bOQ".to_vec(),
            3 => b"\x1bOR".to_vec(),
            4 => b"\x1bOS".to_vec(),
            5 => b"\x1b[15~".to_vec(),
            6 => b"\x1b[17~".to_vec(),
            7 => b"\x1b[18~".to_vec(),
            8 => b"\x1b[19~".to_vec(),
            9 => b"\x1b[20~".to_vec(),
            10 => b"\x1b[21~".to_vec(),
            11 => b"\x1b[23~".to_vec(),
            12 => b"\x1b[24~".to_vec(),
            _ => vec![],
        },
        _ => vec![],
    }
}

fn append_recent_text(recent: &mut String, chunk: &str) {
    recent.push_str(chunk);
    const MAX_RECENT_CHARS: usize = 2048;
    if recent.len() > MAX_RECENT_CHARS {
        let drain_len = recent.len() - MAX_RECENT_CHARS;
        recent.drain(..drain_len);
    }
}

fn looks_like_shell_prompt(recent: &str) -> bool {
    let normalized = recent.replace('\r', "");
    let Some(last_line) = normalized.lines().last() else {
        return false;
    };
    let trimmed = last_line.trim_end();
    !trimmed.is_empty()
        && (trimmed.ends_with('$')
            || trimmed.ends_with('#')
            || trimmed.ends_with('>')
            || trimmed.ends_with('%'))
}

fn looks_like_input_prompt(recent: &str) -> bool {
    let lower = recent.to_ascii_lowercase();
    [
        "[y/n]",
        "[y/N]",
        "[yes/no]",
        "(y/n)",
        "(yes/no)",
        "press enter",
        "continue?",
        "are you sure",
        "password",
        "passphrase",
        "confirmation",
        "confirm",
    ]
    .iter()
    .any(|marker| lower.contains(&marker.to_ascii_lowercase()))
}

pub fn run_command_with_handoff(session_id: &str, command: &str) -> Result<CommandHandoff> {
    let mut reader = ipc_support::connect_streaming(&IpcMessage::AttachSessionLive {
        session_id: session_id.to_string(),
    })?;

    let mut first_line = String::new();
    reader
        .read_line(&mut first_line)
        .map_err(|e| anyhow::anyhow!("Failed to read attach response: {}", e))?;

    let response: IpcMessage = serde_json::from_str(first_line.trim())
        .map_err(|e| anyhow::anyhow!("Failed to parse attach response: {}", e))?;

    match response {
        IpcMessage::Ok => {}
        IpcMessage::Error { message } => bail!("Failed to attach: {}", message),
        _ => bail!("Unexpected response from GUI"),
    }

    let (output_tx, output_rx) = std::sync::mpsc::channel::<IpcMessage>();

    let reader_thread = std::thread::spawn(move || {
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }

                    match serde_json::from_str::<IpcMessage>(trimmed) {
                        Ok(message @ IpcMessage::SessionOutput { .. })
                        | Ok(message @ IpcMessage::SessionEnded { .. }) => {
                            let ended = matches!(&message, IpcMessage::SessionEnded { .. });
                            if output_tx.send(message).is_err() || ended {
                                break;
                            }
                        }
                        Ok(_) => {}
                        Err(_) => {}
                    }
                }
                Err(_) => break,
            }
        }
    });

    match ipc_support::send(&IpcMessage::SendInput {
        session_id: session_id.to_string(),
        data: format!("{}\n", command).into_bytes(),
    })? {
        IpcMessage::Ok => {}
        IpcMessage::Error { message } => anyhow::bail!("{message}"),
        _ => anyhow::bail!("Unexpected command submission response"),
    }

    let start = Instant::now();
    let mut recent = String::new();
    let mut saw_output = false;
    let mut shell_status = ShellCommandStatus::default();

    loop {
        if start.elapsed() >= COMMAND_MAX_DURATION {
            return Ok(CommandHandoff::AwaitingInput);
        }

        match output_rx.recv_timeout(COMMAND_IDLE_TIMEOUT) {
            Ok(IpcMessage::SessionOutput { data, .. }) => {
                saw_output = true;
                shell_status.observe(&data);
                let text = String::from_utf8_lossy(&data);
                print!("{}", text);
                let _ = std::io::stdout().flush();
                append_recent_text(&mut recent, &text);
            }
            Ok(IpcMessage::SessionEnded { reason }) => {
                let _ = reader_thread.join();
                if !reason.is_empty() {
                    eprintln!("\nSession ended: {}", reason);
                }
                return Ok(CommandHandoff::SessionEnded(shell_status.exit_code));
            }
            Ok(_) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if !saw_output {
                    continue;
                }

                if looks_like_input_prompt(&recent) || !looks_like_shell_prompt(&recent) {
                    return Ok(CommandHandoff::AwaitingInput);
                }

                return Ok(CommandHandoff::Completed(shell_status.exit_code));
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                let _ = reader_thread.join();
                return shell_status
                    .exit_code
                    .map(|code| CommandHandoff::Completed(Some(code)))
                    .ok_or_else(|| {
                        anyhow::anyhow!("Lost terminal output before command status was known")
                    });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

    use super::{
        key_event_to_bytes, looks_like_input_prompt, looks_like_shell_prompt, ShellCommandStatus,
    };

    #[test]
    fn shell_status_tracks_only_matching_completed_command_across_chunks() {
        let mut status = ShellCommandStatus::default();
        status.observe(b"\x1b]3008;start=older;type=shell\x1b\\");
        status.observe(b"\x1b]3008;start=abc;type=command;cwd=/home/ubuntu\x1b");
        status.observe(b"\\some output\r\n\x1b]3008;end=unrelated;exit=failure;status=9\x07");
        assert_eq!(status.exit_code, None);
        status.observe(b"\x1b]3008;end=abc;exit=failure;sta");
        status.observe(b"tus=42\x1b\\");
        assert_eq!(status.exit_code, Some(42));
        status.observe(b"\x1b]3008;start=def;type=command\x07");
        assert_eq!(status.exit_code, None);
        status.observe(b"\x1b]3008;end=def;exit=success\x1b\\");
        assert_eq!(status.exit_code, Some(0));
    }

    #[test]
    fn shell_status_never_interprets_a_prompt_as_success() {
        let mut status = ShellCommandStatus::default();
        status.observe(b"$ false\r\n$ ");
        assert_eq!(status.exit_code, None);
        status.observe(b"\x1b]3008;start=1;type=command\x1b\\");
        status.observe(b"\x1b]3008;end=1;exit=failure\x1b\\");
        assert_eq!(status.exit_code, Some(1));
    }

    #[test]
    fn detects_common_shell_prompts() {
        assert!(looks_like_shell_prompt("user@host:~$ "));
        assert!(looks_like_shell_prompt("root@box:/srv# "));
        assert!(looks_like_shell_prompt("PS C:\\Users\\Ricky> "));
    }

    #[test]
    fn detects_common_interactive_prompts() {
        assert!(looks_like_input_prompt("Proceed? [y/N] "));
        assert!(looks_like_input_prompt("Press ENTER to continue"));
        assert!(looks_like_input_prompt("Enter password for deploy: "));
    }

    #[test]
    fn release_events_do_not_generate_input_bytes() {
        let key = KeyEvent::new_with_kind(
            KeyCode::Char('c'),
            KeyModifiers::NONE,
            KeyEventKind::Release,
        );

        assert!(
            key_event_to_bytes(&key).is_empty(),
            "release events should not be forwarded to the remote PTY"
        );
    }

    #[test]
    fn repeat_events_still_generate_input_bytes() {
        let key =
            KeyEvent::new_with_kind(KeyCode::Char('c'), KeyModifiers::NONE, KeyEventKind::Repeat);

        assert_eq!(key_event_to_bytes(&key), b"c".to_vec());
    }
}
