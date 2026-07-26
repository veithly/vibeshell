//! Local shell session management using portable-pty.

use anyhow::{anyhow, Result};
use chrono::Utc;
use log::{debug, error, info};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

use super::ShellInfo;

fn build_shell_command(shell_info: &ShellInfo) -> CommandBuilder {
    let mut cmd = CommandBuilder::new(&shell_info.path);

    match shell_info.id.as_str() {
        "pwsh" | "powershell" => cmd.arg("-NoLogo"),
        "bash" | "zsh" | "fish" | "sh" | "git-bash" | "msys2-bash" | "cygwin-bash" => {
            cmd.arg("-l");
            cmd.arg("-i");
        }
        "cmd" | "wsl" => {}
        _ => {}
    }

    #[cfg(not(target_os = "windows"))]
    {
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("TERM_PROGRAM", "VibeShell");
        cmd.env("SHELL", &shell_info.path);

        if let Some(home) = std::env::var_os("HOME") {
            let home_path = std::path::PathBuf::from(&home);
            if home_path.is_dir() {
                cmd.cwd(home);
            }
        }
    }

    cmd
}

/// State of a local shell session
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LocalShellState {
    Starting,
    Running,
    Stopped,
    Error,
}

/// Information about a local shell session (for frontend)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalShellInfo {
    pub id: String,
    pub shell_id: String,
    pub shell_name: String,
    pub cwd: Option<String>,
    pub agent_id: Option<String>,
    pub state: LocalShellState,
    pub created_at: i64,
    pub clients: usize,
}

/// A local shell session using PTY
pub struct LocalShellSession {
    pub id: String,
    pub shell_id: String,
    pub shell_name: String,
    cwd: Option<PathBuf>,
    agent_id: Option<String>,
    state: Arc<RwLock<LocalShellState>>,
    created_at: i64,

    // PTY writer (separated for thread safety)
    writer: Arc<std::sync::Mutex<Option<Box<dyn Write + Send>>>>,

    // PTY master for resize operations
    master: Arc<std::sync::Mutex<Option<Box<dyn MasterPty + Send>>>>,

    // Child process
    child: Arc<std::sync::Mutex<Option<Box<dyn Child + Send + Sync>>>>,

    // Flag to signal shutdown
    shutdown: Arc<AtomicBool>,

    // Channels for I/O
    output_tx: broadcast::Sender<Vec<u8>>,
    output_bridge_started: Arc<AtomicBool>,

    // Buffered output replay so re-attaching frontends (e.g. after a split pane
    // is recreated) can redraw recent history instead of showing a blank screen.
    // Uses a std Mutex because the reader thread is a sync thread, not a tokio task.
    output_replay: Arc<std::sync::Mutex<crate::replay::OutputReplayBuffer>>,

    // Track connected clients
    client_count: Arc<AtomicUsize>,
}

impl LocalShellSession {
    /// Create a new local shell session
    pub fn new(shell_info: &ShellInfo, cols: u16, rows: u16) -> Result<Self> {
        info!(
            "[LocalShell] Creating session for shell: {} ({})",
            shell_info.name, shell_info.path
        );

        let command = build_shell_command(shell_info);
        Self::spawn(
            shell_info.id.clone(),
            shell_info.name.clone(),
            None,
            None,
            command,
            cols,
            rows,
        )
    }

    /// Create a PTY session for a known local process, such as a coding agent.
    pub fn new_process(
        shell_id: String,
        shell_name: String,
        cwd: Option<PathBuf>,
        agent_id: Option<String>,
        command: CommandBuilder,
        cols: u16,
        rows: u16,
    ) -> Result<Self> {
        info!("[LocalShell] Creating process session: {}", shell_name);
        Self::spawn(shell_id, shell_name, cwd, agent_id, command, cols, rows)
    }

    fn spawn(
        shell_id: String,
        shell_name: String,
        cwd: Option<PathBuf>,
        agent_id: Option<String>,
        command: CommandBuilder,
        cols: u16,
        rows: u16,
    ) -> Result<Self> {
        // Create the PTY system
        let pty_system = native_pty_system();

        // Create PTY pair with initial size
        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        // Spawn the shell or process.
        let child = pair.slave.spawn_command(command)?;

        info!("[LocalShell] Shell process spawned successfully");

        // Get the writer from master
        let writer = pair.master.take_writer()?;

        // Create output channel
        let (output_tx, _) = broadcast::channel::<Vec<u8>>(256);

        // Create reader from master
        let reader = pair.master.try_clone_reader()?;

        let session = Self {
            id: Uuid::new_v4().to_string(),
            shell_id,
            shell_name,
            cwd,
            agent_id,
            state: Arc::new(RwLock::new(LocalShellState::Running)),
            created_at: Utc::now().timestamp(),
            writer: Arc::new(std::sync::Mutex::new(Some(writer))),
            master: Arc::new(std::sync::Mutex::new(Some(pair.master))),
            child: Arc::new(std::sync::Mutex::new(Some(child))),
            shutdown: Arc::new(AtomicBool::new(false)),
            output_tx,
            output_bridge_started: Arc::new(AtomicBool::new(false)),
            output_replay: Arc::new(std::sync::Mutex::new(
                crate::replay::OutputReplayBuffer::default(),
            )),
            client_count: Arc::new(AtomicUsize::new(0)),
        };

        // Reap the child independently from PTY output. The monitor only holds
        // the child mutex for non-blocking polls, leaving stop() free to kill it.
        session.start_child_monitor();
        session.start_reader(reader);

        Ok(session)
    }

    fn start_child_monitor(&self) {
        let session_id = self.id.clone();
        let child = self.child.clone();
        let state = self.state.clone();

        thread::spawn(move || loop {
            let poll_result = {
                let mut child_guard = match child.lock() {
                    Ok(guard) => guard,
                    Err(error) => {
                        error!(
                            "[LocalShell] Failed to lock child monitor for session {}: {}",
                            session_id, error
                        );
                        return;
                    }
                };
                let Some(child_process) = child_guard.as_mut() else {
                    return;
                };
                match child_process.try_wait() {
                    Ok(Some(status)) => {
                        *child_guard = None;
                        Ok(Some(status))
                    }
                    Ok(None) => Ok(None),
                    Err(error) => Err(error),
                }
            };

            match poll_result {
                Ok(Some(status)) => {
                    let mut state_guard = state.blocking_write();
                    if *state_guard == LocalShellState::Running {
                        *state_guard = LocalShellState::Stopped;
                    }
                    drop(state_guard);
                    info!(
                        "[LocalShell] Child process exited for session {}: {:?}",
                        session_id, status
                    );
                    return;
                }
                Ok(None) => thread::sleep(std::time::Duration::from_millis(25)),
                Err(error) => {
                    error!(
                        "[LocalShell] Failed to poll child for session {}: {}",
                        session_id, error
                    );
                    return;
                }
            }
        });
    }

    /// Start the reader thread for PTY output
    fn start_reader(&self, mut reader: Box<dyn Read + Send>) {
        let session_id = self.id.clone();
        let output_tx = self.output_tx.clone();
        let shutdown = self.shutdown.clone();
        let state = self.state.clone();
        let output_replay = self.output_replay.clone();

        thread::spawn(move || {
            debug!(
                "[LocalShell] Output reader thread started for session {}",
                session_id
            );

            let mut buf = [0u8; 4096];
            loop {
                // Check for shutdown
                if shutdown.load(Ordering::Relaxed) {
                    debug!(
                        "[LocalShell] Shutdown signal received for session {}",
                        session_id
                    );
                    break;
                }

                match reader.read(&mut buf) {
                    Ok(0) => {
                        info!("[LocalShell] PTY EOF for session {}", session_id);
                        break;
                    }
                    Ok(n) => {
                        let data = buf[..n].to_vec();
                        // Buffer output for late-attaching frontends (pane recreate).
                        if let Ok(mut replay) = output_replay.lock() {
                            replay.push(&data);
                        }
                        // Ignore send errors (no receivers)
                        let _ = output_tx.send(data);
                    }
                    Err(e) => {
                        if shutdown.load(Ordering::Relaxed) {
                            debug!(
                                "[LocalShell] Session {} stopped, exiting reader",
                                session_id
                            );
                        } else {
                            error!(
                                "[LocalShell] PTY read error for session {}: {}",
                                session_id, e
                            );
                        }
                        break;
                    }
                }
            }

            // This is a plain reader thread, so it has no ambient Tokio runtime.
            // Update the async lock through its blocking API before the thread exits.
            let mut state_guard = state.blocking_write();
            if *state_guard == LocalShellState::Running {
                *state_guard = LocalShellState::Stopped;
            }
            drop(state_guard);

            info!(
                "[LocalShell] Output reader thread ended for session {}",
                session_id
            );
        });
    }

    /// Get session info for frontend
    pub async fn get_info(&self) -> LocalShellInfo {
        LocalShellInfo {
            id: self.id.clone(),
            shell_id: self.shell_id.clone(),
            shell_name: self.shell_name.clone(),
            cwd: self
                .cwd
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned()),
            agent_id: self.agent_id.clone(),
            state: self.get_state().await,
            created_at: self.created_at,
            clients: self.client_count.load(Ordering::Relaxed),
        }
    }

    /// Get current session state
    pub async fn get_state(&self) -> LocalShellState {
        self.state.read().await.clone()
    }

    /// Set session state
    pub async fn set_state(&self, new_state: LocalShellState) {
        let mut state = self.state.write().await;
        *state = new_state;
    }

    /// Subscribe to output
    pub fn subscribe(&self) -> broadcast::Receiver<Vec<u8>> {
        self.output_tx.subscribe()
    }

    /// Claim the single application-level output bridge for this session.
    pub fn claim_output_bridge(&self) -> bool {
        self.output_bridge_started
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    /// Snapshot of buffered output for late-attaching frontends.
    pub fn replay_output(&self) -> Vec<Vec<u8>> {
        self.output_replay
            .lock()
            .map(|replay| replay.snapshot())
            .unwrap_or_default()
    }

    /// Get output sender for bridging
    pub fn output_sender(&self) -> broadcast::Sender<Vec<u8>> {
        self.output_tx.clone()
    }

    /// Send input to the shell
    pub fn write_input(&self, data: &[u8]) -> Result<()> {
        let mut writer_guard = self
            .writer
            .lock()
            .map_err(|e| anyhow!("Failed to lock writer: {}", e))?;

        if let Some(ref mut writer) = *writer_guard {
            writer.write_all(data)?;
            writer.flush()?;
            Ok(())
        } else {
            Err(anyhow!("Writer not available"))
        }
    }

    /// Resize the PTY
    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        let master_guard = self
            .master
            .lock()
            .map_err(|e| anyhow!("Failed to lock master: {}", e))?;

        if let Some(ref master) = *master_guard {
            master.resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })?;
            debug!(
                "[LocalShell] Resized session {} to {}x{}",
                self.id, cols, rows
            );
        }
        Ok(())
    }

    /// Attach a client
    pub fn attach(&self) {
        self.client_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Detach a client
    pub fn detach(&self) {
        let _ = self
            .client_count
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current.checked_sub(1)
            });
    }

    /// Get client count
    pub fn client_count(&self) -> usize {
        self.client_count.load(Ordering::Relaxed)
    }

    /// Stop the shell session
    pub async fn stop(&self) -> Result<()> {
        info!("[LocalShell] Stopping session {}", self.id);

        // Signal shutdown
        self.shutdown.store(true, Ordering::Relaxed);

        // Mark as stopped
        self.set_state(LocalShellState::Stopped).await;

        // Clear writer
        {
            let mut writer_guard = self
                .writer
                .lock()
                .map_err(|e| anyhow!("Failed to lock writer: {}", e))?;
            *writer_guard = None;
        }

        // Child::kill preserves portable-pty's platform-specific termination
        // semantics (including Unix escalation). The monitor never blocks while
        // holding this mutex, so stop can always acquire it.
        let kill_error = {
            let mut child_guard = self
                .child
                .lock()
                .map_err(|e| anyhow!("Failed to lock child: {}", e))?;
            child_guard.as_mut().and_then(|child| child.kill().err())
        };

        // Closing the PTY wakes the reader if it is still blocked in read(); the
        // child monitor remains responsible for reaping the process.
        {
            let mut master_guard = self
                .master
                .lock()
                .map_err(|e| anyhow!("Failed to lock master: {}", e))?;
            *master_guard = None;
        }

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            let exited = {
                let mut child_guard = self
                    .child
                    .lock()
                    .map_err(|e| anyhow!("Failed to lock child: {}", e))?;
                let Some(child) = child_guard.as_mut() else {
                    return Ok(());
                };
                match child.try_wait() {
                    Ok(Some(_)) => {
                        *child_guard = None;
                        true
                    }
                    Ok(None) => false,
                    Err(error) => return Err(error.into()),
                }
            };

            if exited {
                info!("[LocalShell] Child process killed for session {}", self.id);
                return Ok(());
            }
            if std::time::Instant::now() >= deadline {
                if let Some(error) = kill_error {
                    return Err(error.into());
                }
                return Err(anyhow!(
                    "Child process did not exit after termination for session {}",
                    self.id
                ));
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_shell::detector::ShellType;
    use std::ffi::OsStr;
    use std::time::{Duration, Instant};

    #[test]
    fn zsh_launch_is_explicitly_interactive_and_login() {
        let shell = ShellInfo {
            id: "zsh".to_string(),
            name: "Zsh".to_string(),
            path: "/bin/zsh".to_string(),
            shell_type: ShellType::Zsh,
            is_default: true,
        };

        let command = build_shell_command(&shell);
        assert_eq!(command.get_argv(), &["/bin/zsh", "-l", "-i"]);

        #[cfg(not(target_os = "windows"))]
        {
            assert_eq!(command.get_env("TERM"), Some(OsStr::new("xterm-256color")));
            assert_eq!(command.get_env("COLORTERM"), Some(OsStr::new("truecolor")));
            assert_eq!(
                command.get_env("TERM_PROGRAM"),
                Some(OsStr::new("VibeShell"))
            );
            assert_eq!(command.get_env("SHELL"), Some(OsStr::new("/bin/zsh")));
        }
    }

    #[tokio::test]
    async fn short_lived_process_transitions_to_stopped() {
        #[cfg(target_os = "windows")]
        let command = {
            let mut command = CommandBuilder::new("cmd.exe");
            command.args(["/C", "exit", "0"]);
            command
        };
        #[cfg(not(target_os = "windows"))]
        let command = {
            let mut command = CommandBuilder::new("/bin/sh");
            command.args(["-c", "exit 0"]);
            command
        };

        let session = LocalShellSession::new_process(
            "short-lived".into(),
            "Short lived".into(),
            None,
            Some("test-agent".into()),
            command,
            80,
            24,
        )
        .unwrap();

        let deadline = Instant::now() + Duration::from_secs(2);
        while (session.get_state().await == LocalShellState::Running
            || session.child.lock().unwrap().is_some())
            && Instant::now() < deadline
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        assert_eq!(session.get_state().await, LocalShellState::Stopped);
        assert!(session.child.lock().unwrap().is_none());

        session.detach();
        assert_eq!(session.client_count(), 0);
        session.attach();
        session.detach();
        session.detach();
        assert_eq!(session.client_count(), 0);
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn stop_returns_after_pty_eof_while_child_is_still_alive() {
        let mut command = CommandBuilder::new("/bin/sh");
        command.args([
            "-c",
            "trap '' HUP; /bin/sleep 0.1; exec /bin/sleep 5 </dev/null >/dev/null 2>&1",
        ]);
        command.set_controlling_tty(false);

        let session = Arc::new(
            LocalShellSession::new_process(
                "eof-before-exit".into(),
                "EOF before exit".into(),
                None,
                Some("test-agent".into()),
                command,
                80,
                24,
            )
            .unwrap(),
        );
        let child_pid = session
            .child
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|child| child.process_id())
            .expect("spawned child should expose a process ID");
        let process_exists = |pid: u32| {
            std::process::Command::new("kill")
                .arg("-0")
                .arg(pid.to_string())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok_and(|status| status.success())
        };

        let eof_deadline = Instant::now() + Duration::from_secs(2);
        while session.get_state().await == LocalShellState::Running && Instant::now() < eof_deadline
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(session.get_state().await, LocalShellState::Stopped);
        assert!(process_exists(child_pid));

        let stop_session = session.clone();
        let stop_task = tokio::spawn(async move { stop_session.stop().await });
        tokio::time::timeout(Duration::from_secs(3), stop_task)
            .await
            .expect("stop blocked behind the reader's child wait")
            .expect("stop task panicked")
            .expect("stop failed");

        let reap_deadline = Instant::now() + Duration::from_secs(2);
        while process_exists(child_pid) && Instant::now() < reap_deadline {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(!process_exists(child_pid), "child process was not reaped");
        assert!(session.child.lock().unwrap().is_none());
    }
}
