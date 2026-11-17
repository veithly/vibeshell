use super::{process::cancelled, TeleportTarget};
use anyhow::{anyhow, bail, Context, Result};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize};
use std::{
    io::{Read, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::sync::{mpsc, oneshot};

type PtyChild = Arc<Mutex<Option<Box<dyn Child + Send + Sync>>>>;
type PtyWriter = Arc<Mutex<Box<dyn Write + Send>>>;
type PtyMaster = Arc<Mutex<Box<dyn MasterPty + Send>>>;

pub struct TeleportPty {
    writer: PtyWriter,
    master: PtyMaster,
    child: PtyChild,
    alive: Arc<AtomicBool>,
    target: TeleportTarget,
}

impl TeleportPty {
    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }

    pub async fn write(&self, data: &[u8]) -> Result<()> {
        if !self.is_alive() {
            bail!("Teleport session is closed");
        }
        if data.len() > 1024 * 1024 {
            bail!("Teleport terminal input exceeds 1 MiB");
        }
        let writer = self.writer.clone();
        let data = data.to_vec();
        let task = tokio::task::spawn_blocking(move || -> Result<()> {
            let mut writer = writer
                .lock()
                .map_err(|_| anyhow!("PTY writer lock poisoned"))?;
            writer.write_all(&data)?;
            writer.flush()?;
            Ok(())
        });
        tokio::select! {
            result = tokio::time::timeout(Duration::from_secs(10), task) => match result {
                Ok(result) => result.context("PTY writer task failed")?,
                Err(_) => { self.kill(); Err(anyhow!("Teleport input timed out; the session was closed, input was not replayed")) }
            },
            _ = cancelled(self.target.cancellation()) => { self.kill(); Err(anyhow!("Teleport input cancelled")) }
        }
    }

    pub fn resize(&self, cols: u32, rows: u32) -> Result<()> {
        let (cols, rows) = dimensions(cols, rows)?;
        if !self.is_alive() {
            bail!("Teleport session is closed");
        }
        self.master
            .lock()
            .map_err(|_| anyhow!("PTY master lock poisoned"))?
            .resize(PtySize {
                cols,
                rows,
                pixel_width: 0,
                pixel_height: 0,
            })?;
        Ok(())
    }

    pub fn kill(&self) {
        self.target.cancel();
        // The monitor only uses try_wait. Killing never waits for a reader or
        // a writer mutex, so blocked output/input cannot deadlock teardown.
        if let Ok(mut child) = self.child.lock() {
            if let Some(child) = child.as_mut() {
                let _ = child.kill();
            }
        }
        self.alive.store(false, Ordering::Release);
    }
}
impl Drop for TeleportPty {
    fn drop(&mut self) {
        self.kill();
    }
}

fn dimensions(cols: u32, rows: u32) -> Result<(u16, u16)> {
    if cols == 0 || rows == 0 {
        bail!("Terminal dimensions must be positive");
    }
    Ok((u16::try_from(cols)?, u16::try_from(rows)?))
}

/// This marker is produced by an authenticated remote exec request, never
/// submitted as terminal input. Echoed input and local prompts cannot mark a
/// failed authentication as a connected shell. Marker bytes never reach xterm.
pub async fn spawn_tsh_ssh(
    target: &TeleportTarget,
    cols: u32,
    rows: u32,
    output: mpsc::Sender<Vec<u8>>,
) -> Result<TeleportPty> {
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    let marker = format!("\x1b]999;vibeshell-ready;{nonce}\x07").into_bytes();
    let startup = format!(
        "printf '\\033]999;vibeshell-ready;{nonce}\\007'; exec \"${{SHELL:-/bin/sh}}\" -il"
    );
    let mut command = CommandBuilder::new(&target.binary);
    command.args(target.ssh_args(true, &startup));
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    connect_command(
        command,
        target.clone(),
        cols,
        rows,
        marker,
        output,
        Duration::from_secs(30),
    )
    .await
}

async fn connect_command(
    command: CommandBuilder,
    target: TeleportTarget,
    cols: u32,
    rows: u32,
    marker: Vec<u8>,
    output: mpsc::Sender<Vec<u8>>,
    timeout: Duration,
) -> Result<TeleportPty> {
    let (cols, rows) = dimensions(cols, rows)?;
    if *target.cancellation().borrow() {
        bail!("Teleport connection cancelled");
    }
    let signal = target.cancellation();
    let (pty, ready) = tokio::task::spawn_blocking(move || {
        spawn_process(command, target, cols, rows, marker, output)
    })
    .await??;
    let result = tokio::select! {
        result = tokio::time::timeout(timeout, ready) => match result {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(anyhow!("Teleport closed before confirming the authenticated shell")),
            Err(_) => Err(anyhow!("Teleport authentication/shell timed out; run tsh login explicitly before connecting")),
        },
        _ = cancelled(signal) => Err(anyhow!("Teleport exited or was cancelled before the shell was ready")),
    };
    result?; // TeleportPty::drop terminates the child on every failed path.
    if !pty.is_alive() {
        bail!("Teleport shell exited during startup");
    }
    Ok(pty)
}

fn spawn_process(
    command: CommandBuilder,
    target: TeleportTarget,
    cols: u16,
    rows: u16,
    marker: Vec<u8>,
    output: mpsc::Sender<Vec<u8>>,
) -> Result<(TeleportPty, oneshot::Receiver<Result<()>>)> {
    let pair = portable_pty::native_pty_system().openpty(PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    })?;
    // Acquire fallible handles before spawning so an initialization error
    // cannot leave a child running without an owner.
    let writer = Arc::new(Mutex::new(pair.master.take_writer()?));
    let mut reader = pair.master.try_clone_reader()?;
    let child = Arc::new(Mutex::new(Some(pair.slave.spawn_command(command)?)));
    drop(pair.slave);
    let alive = Arc::new(AtomicBool::new(true));
    let pty = TeleportPty {
        writer,
        master: Arc::new(Mutex::new(pair.master)),
        child: child.clone(),
        alive: alive.clone(),
        target: target.clone(),
    };
    let monitor_target = target.clone();
    std::thread::spawn(move || loop {
        let finished = {
            let mut guard = match child.lock() {
                Ok(guard) => guard,
                Err(_) => break,
            };
            match guard.as_mut() {
                Some(child) => match child.try_wait() {
                    Ok(Some(_)) => {
                        *guard = None;
                        true
                    }
                    Ok(None) => false,
                    Err(_) => {
                        if let Some(child) = guard.as_mut() {
                            let _ = child.kill();
                        }
                        true
                    }
                },
                None => true,
            }
        };
        if finished {
            alive.store(false, Ordering::Release);
            monitor_target.cancel();
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    });
    let (ready, receiver) = oneshot::channel();
    let child = pty.child.clone();
    let alive = pty.alive.clone();
    std::thread::spawn(move || {
        let mut handshake = Handshake::new(marker);
        let mut ready = Some(ready);
        let mut chunk = [0; 8192];
        loop {
            let n = match reader.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            let data = match handshake.push(&chunk[..n]) {
                Ok(data) => data,
                Err(error) => {
                    if let Some(ready) = ready.take() {
                        let _ = ready.send(Err(error));
                    }
                    break;
                }
            };
            if handshake.ready {
                if let Some(ready) = ready.take() {
                    if ready.send(Ok(())).is_err() {
                        break;
                    }
                }
            }
            if let Some(data) = data {
                if !data.is_empty() && output.blocking_send(data).is_err() {
                    break;
                }
            }
        }
        if let Some(ready) = ready {
            let _ = ready.send(Err(anyhow!(
                "Teleport authentication failed or shell closed before readiness"
            )));
        }
        target.cancel();
        if let Ok(mut guard) = child.lock() {
            if let Some(child) = guard.as_mut() {
                let _ = child.kill();
            }
        }
        alive.store(false, Ordering::Release);
    });
    Ok((pty, receiver))
}

struct Handshake {
    marker: Vec<u8>,
    pending: Vec<u8>,
    ready: bool,
}
impl Handshake {
    fn new(marker: Vec<u8>) -> Self {
        Self {
            marker,
            pending: Vec::new(),
            ready: false,
        }
    }
    fn push(&mut self, data: &[u8]) -> Result<Option<Vec<u8>>> {
        if self.ready {
            return Ok(Some(data.to_vec()));
        }
        self.pending.extend_from_slice(data);
        if self.pending.len() > 64 * 1024 {
            bail!("Teleport startup output exceeded 64 KiB without authentication readiness");
        }
        if let Some(index) = self
            .pending
            .windows(self.marker.len())
            .position(|window| window == self.marker)
        {
            self.pending.drain(index..index + self.marker.len());
            self.ready = true;
            return Ok(Some(std::mem::take(&mut self.pending)));
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn handshake_is_binary_safe_fragment_safe_and_bounded() {
        let marker = b"\x1b]999;nonce\x07";
        for split in 0..=marker.len() {
            let mut h = Handshake::new(marker.to_vec());
            let mut bytes = vec![255];
            bytes.extend_from_slice(&marker[..split]);
            let first = h.push(&bytes).unwrap();
            let mut tail = marker[split..].to_vec();
            tail.extend_from_slice(b"prompt");
            let second = h.push(&tail).unwrap();
            assert_eq!(
                first
                    .into_iter()
                    .flatten()
                    .chain(second.into_iter().flatten())
                    .collect::<Vec<_>>(),
                b"\xffprompt"
            );
            assert!(h.ready);
        }
        assert!(Handshake::new(marker.to_vec())
            .push(&vec![0; 65537])
            .is_err());
        assert!(dimensions(65536, 24).is_err());
        assert!(dimensions(80, 0).is_err());
    }

    fn command(code: &str) -> CommandBuilder {
        let mut command = CommandBuilder::new(if cfg!(windows) { "python" } else { "python3" });
        command.args(["-c", code]);
        command
    }
    #[tokio::test]
    async fn actual_process_readiness_auth_failure_timeout_and_eof() {
        let target = TeleportTarget::new("proxy", "u", "node").unwrap();
        let (tx, mut rx) = mpsc::channel(16);
        let pty = connect_command(
            command("import sys; sys.stdout.write('READY'); sys.stdout.flush(); input();"),
            target,
            80,
            24,
            b"READY".to_vec(),
            tx,
            Duration::from_secs(5),
        )
        .await
        .unwrap();
        assert!(pty.is_alive());
        pty.resize(100, 30).unwrap();
        pty.write(b"hello\n").await.unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while pty.is_alive() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        while let Ok(data) = rx.try_recv() {
            assert!(!String::from_utf8_lossy(&data).contains("READY"));
        }
        for (code, timeout) in [
            (
                "import sys; sys.stderr.write('denied'); sys.exit(7)",
                Duration::from_secs(5),
            ),
            ("import time; time.sleep(30)", Duration::from_millis(150)),
        ] {
            let target = TeleportTarget::new("proxy", "u", "node").unwrap();
            let (tx, _rx) = mpsc::channel(16);
            assert!(connect_command(
                command(code),
                target,
                80,
                24,
                b"READY".to_vec(),
                tx,
                timeout
            )
            .await
            .is_err());
        }
    }
}
