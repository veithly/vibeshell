//! Local shell session management using portable-pty.

use anyhow::{Result, anyhow};
use chrono::Utc;
use portable_pty::{CommandBuilder, PtySize, native_pty_system, Child, MasterPty};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;
use log::{debug, error, info};

use super::ShellInfo;

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
    pub state: LocalShellState,
    pub created_at: i64,
    pub clients: usize,
}

/// A local shell session using PTY
pub struct LocalShellSession {
    pub id: String,
    pub shell_id: String,
    pub shell_name: String,
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

    // Track connected clients
    client_count: Arc<AtomicUsize>,
}

impl LocalShellSession {
    /// Create a new local shell session
    pub fn new(
        shell_info: &ShellInfo,
        cols: u16,
        rows: u16,
    ) -> Result<Self> {
        info!("[LocalShell] Creating session for shell: {} ({})", shell_info.name, shell_info.path);

        // Create the PTY system
        let pty_system = native_pty_system();

        // Create PTY pair with initial size
        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        // Build command for the shell
        let mut cmd = CommandBuilder::new(&shell_info.path);

        // Add shell-specific arguments for interactive mode
        match shell_info.id.as_str() {
            "pwsh" | "powershell" => {
                cmd.arg("-NoLogo");
            }
            "cmd" => {
                // cmd doesn't need extra args
            }
            "bash" | "zsh" | "fish" | "sh" | "git-bash" | "msys2-bash" | "cygwin-bash" => {
                cmd.arg("-l"); // Login shell
            }
            "wsl" => {
                // WSL starts with default shell
            }
            _ => {}
        }

        // Spawn the shell process
        let child = pair.slave.spawn_command(cmd)?;

        info!("[LocalShell] Shell process spawned successfully");

        // Get the writer from master
        let writer = pair.master.take_writer()?;

        // Create output channel
        let (output_tx, _) = broadcast::channel::<Vec<u8>>(256);

        // Create reader from master
        let reader = pair.master.try_clone_reader()?;

        let session = Self {
            id: Uuid::new_v4().to_string(),
            shell_id: shell_info.id.clone(),
            shell_name: shell_info.name.clone(),
            state: Arc::new(RwLock::new(LocalShellState::Running)),
            created_at: Utc::now().timestamp(),
            writer: Arc::new(std::sync::Mutex::new(Some(writer))),
            master: Arc::new(std::sync::Mutex::new(Some(pair.master))),
            child: Arc::new(std::sync::Mutex::new(Some(child))),
            shutdown: Arc::new(AtomicBool::new(false)),
            output_tx,
            client_count: Arc::new(AtomicUsize::new(0)),
        };

        // Start the reader thread
        session.start_reader(reader);

        Ok(session)
    }

    /// Start the reader thread for PTY output
    fn start_reader(&self, mut reader: Box<dyn Read + Send>) {
        let session_id = self.id.clone();
        let output_tx = self.output_tx.clone();
        let shutdown = self.shutdown.clone();
        let state = self.state.clone();

        thread::spawn(move || {
            debug!("[LocalShell] Output reader thread started for session {}", session_id);

            let mut buf = [0u8; 4096];
            loop {
                // Check for shutdown
                if shutdown.load(Ordering::Relaxed) {
                    debug!("[LocalShell] Shutdown signal received for session {}", session_id);
                    break;
                }

                match reader.read(&mut buf) {
                    Ok(0) => {
                        info!("[LocalShell] PTY EOF for session {}", session_id);
                        break;
                    }
                    Ok(n) => {
                        let data = buf[..n].to_vec();
                        // Ignore send errors (no receivers)
                        let _ = output_tx.send(data);
                    }
                    Err(e) => {
                        if shutdown.load(Ordering::Relaxed) {
                            debug!("[LocalShell] Session {} stopped, exiting reader", session_id);
                        } else {
                            error!("[LocalShell] PTY read error for session {}: {}", session_id, e);
                        }
                        break;
                    }
                }
            }

            // Mark session as stopped using tokio runtime
            let state_clone = state.clone();
            let session_id_clone = session_id.clone();
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move {
                    let mut state_guard = state_clone.write().await;
                    if *state_guard == LocalShellState::Running {
                        *state_guard = LocalShellState::Stopped;
                    }
                });
            }

            info!("[LocalShell] Output reader thread ended for session {}", session_id_clone);
        });
    }

    /// Get session info for frontend
    pub async fn get_info(&self) -> LocalShellInfo {
        LocalShellInfo {
            id: self.id.clone(),
            shell_id: self.shell_id.clone(),
            shell_name: self.shell_name.clone(),
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

    /// Get output sender for bridging
    pub fn output_sender(&self) -> broadcast::Sender<Vec<u8>> {
        self.output_tx.clone()
    }

    /// Send input to the shell
    pub fn write_input(&self, data: &[u8]) -> Result<()> {
        let mut writer_guard = self.writer.lock()
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
        let master_guard = self.master.lock()
            .map_err(|e| anyhow!("Failed to lock master: {}", e))?;

        if let Some(ref master) = *master_guard {
            master.resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })?;
            debug!("[LocalShell] Resized session {} to {}x{}", self.id, cols, rows);
        }
        Ok(())
    }

    /// Attach a client
    pub fn attach(&self) {
        self.client_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Detach a client
    pub fn detach(&self) {
        let current = self.client_count.load(Ordering::Relaxed);
        if current > 0 {
            self.client_count.fetch_sub(1, Ordering::Relaxed);
        }
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
            let mut writer_guard = self.writer.lock()
                .map_err(|e| anyhow!("Failed to lock writer: {}", e))?;
            *writer_guard = None;
        }

        // Kill the child process
        {
            let mut child_guard = self.child.lock()
                .map_err(|e| anyhow!("Failed to lock child: {}", e))?;
            if let Some(ref mut child) = *child_guard {
                child.kill()?;
                info!("[LocalShell] Child process killed for session {}", self.id);
            }
            *child_guard = None;
        }

        // Clear the master
        {
            let mut master_guard = self.master.lock()
                .map_err(|e| anyhow!("Failed to lock master: {}", e))?;
            *master_guard = None;
        }

        Ok(())
    }
}
