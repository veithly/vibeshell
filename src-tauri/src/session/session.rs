use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc, Mutex, RwLock};
use uuid::Uuid;

use crate::ssh::{ClientHandler, SshClient};

const OUTPUT_REPLAY_LIMIT_BYTES: usize = 64 * 1024;

#[derive(Default)]
struct OutputReplayBuffer {
    chunks: VecDeque<Vec<u8>>,
    total_bytes: usize,
}

impl OutputReplayBuffer {
    fn push(&mut self, data: &[u8]) {
        if data.is_empty() {
            return;
        }

        let stored = if data.len() > OUTPUT_REPLAY_LIMIT_BYTES {
            data[data.len() - OUTPUT_REPLAY_LIMIT_BYTES..].to_vec()
        } else {
            data.to_vec()
        };

        self.total_bytes += stored.len();
        self.chunks.push_back(stored);

        while self.total_bytes > OUTPUT_REPLAY_LIMIT_BYTES {
            if let Some(removed) = self.chunks.pop_front() {
                self.total_bytes = self.total_bytes.saturating_sub(removed.len());
            } else {
                self.total_bytes = 0;
                break;
            }
        }
    }

    fn snapshot(&self) -> Vec<Vec<u8>> {
        self.chunks.iter().cloned().collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Connecting,
    Connected,
    Disconnected,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub server_id: String,
    pub server_name: String,
    pub state: SessionState,
    pub created_at: i64,
    pub clients: usize,
}

pub struct Session {
    pub id: String,
    pub server_id: String,
    pub server_name: String,
    state: Arc<RwLock<SessionState>>,
    created_at: i64,

    // SSH client for this session
    ssh_client: Arc<Mutex<Option<SshClient>>>,

    // Channel for sending input to SSH
    input_tx: mpsc::Sender<Vec<u8>>,

    // Broadcast channel for output to all clients
    output_tx: broadcast::Sender<Vec<u8>>,

    // Buffered output replay so late listeners still receive the initial shell prompt.
    output_replay: Arc<Mutex<OutputReplayBuffer>>,

    // Ensure we only spawn one app output forwarder per session.
    output_forwarder_started: Arc<Mutex<bool>>,

    // Track connected clients
    client_count: Arc<RwLock<usize>>,

    // Last observed activity (attach/input/output/resize/exec).
    last_activity: Arc<RwLock<Instant>>,
}

impl Session {
    pub fn new(
        server_id: String,
        server_name: String,
        input_tx: mpsc::Sender<Vec<u8>>,
        output_tx: broadcast::Sender<Vec<u8>>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            server_id,
            server_name,
            state: Arc::new(RwLock::new(SessionState::Connecting)),
            created_at: Utc::now().timestamp(),
            ssh_client: Arc::new(Mutex::new(None)),
            input_tx,
            output_tx,
            output_replay: Arc::new(Mutex::new(OutputReplayBuffer::default())),
            output_forwarder_started: Arc::new(Mutex::new(false)),
            client_count: Arc::new(RwLock::new(0)),
            last_activity: Arc::new(RwLock::new(Instant::now())),
        }
    }

    pub async fn mark_activity(&self) {
        let mut last_activity = self.last_activity.write().await;
        *last_activity = Instant::now();
    }

    pub async fn idle_for(&self) -> Duration {
        let last_activity = self.last_activity.read().await;
        last_activity.elapsed()
    }

    pub async fn should_reap(&self, max_idle: Duration) -> bool {
        self.client_count().await == 0 && self.idle_for().await >= max_idle
    }

    /// Set the SSH client for this session
    pub async fn set_ssh_client(&self, client: SshClient) {
        let mut ssh_guard = self.ssh_client.lock().await;
        *ssh_guard = Some(client);
    }

    /// Get a reference to the output broadcast sender
    pub fn output_sender(&self) -> broadcast::Sender<Vec<u8>> {
        self.output_tx.clone()
    }

    pub async fn set_state(&self, state: SessionState) {
        let mut s = self.state.write().await;
        *s = state;
    }

    pub async fn get_state(&self) -> SessionState {
        self.state.read().await.clone()
    }

    pub async fn get_info(&self) -> SessionInfo {
        SessionInfo {
            id: self.id.clone(),
            server_id: self.server_id.clone(),
            server_name: self.server_name.clone(),
            state: self.get_state().await,
            created_at: self.created_at,
            clients: *self.client_count.read().await,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Vec<u8>> {
        self.output_tx.subscribe()
    }

    pub async fn replay_output(&self) -> Vec<Vec<u8>> {
        let replay = self.output_replay.lock().await;
        replay.snapshot()
    }

    pub async fn publish_output(&self, data: Vec<u8>) {
        {
            let mut replay = self.output_replay.lock().await;
            replay.push(&data);
        }

        self.mark_activity().await;
        let _ = self.output_tx.send(data);
    }

    pub async fn try_start_output_forwarder(&self) -> bool {
        let mut started = self.output_forwarder_started.lock().await;
        if *started {
            false
        } else {
            *started = true;
            true
        }
    }

    pub async fn send_input(&self, data: Vec<u8>) -> Result<()> {
        self.mark_activity().await;
        self.input_tx.send(data).await?;
        Ok(())
    }

    pub async fn attach(&self) {
        let mut count = self.client_count.write().await;
        *count += 1;
        drop(count);
        self.mark_activity().await;
    }

    pub async fn detach(&self) {
        let mut count = self.client_count.write().await;
        if *count > 0 {
            *count -= 1;
        }
        drop(count);
        self.mark_activity().await;
    }

    pub async fn client_count(&self) -> usize {
        *self.client_count.read().await
    }

    /// Send data to the SSH shell
    pub async fn write_to_ssh(&self, data: &[u8]) -> Result<()> {
        let ssh_guard = self.ssh_client.lock().await;
        if let Some(ref client) = *ssh_guard {
            client.send_data(data).await?;
        } else {
            return Err(anyhow::anyhow!("SSH client not connected"));
        }
        drop(ssh_guard);
        self.mark_activity().await;
        Ok(())
    }

    /// Resize the PTY
    pub async fn resize_pty(&self, cols: u32, rows: u32) -> Result<()> {
        let ssh_guard = self.ssh_client.lock().await;
        if let Some(ref client) = *ssh_guard {
            client.resize_pty(cols, rows).await?;
        } else {
            return Err(anyhow::anyhow!("SSH client not connected"));
        }
        drop(ssh_guard);
        self.mark_activity().await;
        Ok(())
    }

    /// Disconnect the SSH session
    pub async fn disconnect(&self) -> Result<()> {
        let mut ssh_guard = self.ssh_client.lock().await;
        if let Some(ref mut client) = *ssh_guard {
            client.disconnect().await?;
        }
        *ssh_guard = None;
        self.set_state(SessionState::Disconnected).await;
        Ok(())
    }

    /// Get the SSH session handle Arc for tunnel/forwarding operations
    pub async fn get_ssh_handle_arc(
        &self,
    ) -> Option<Arc<tokio::sync::Mutex<Option<russh::client::Handle<ClientHandler>>>>> {
        let ssh_guard = self.ssh_client.lock().await;
        (*ssh_guard).as_ref().map(|client| client.session_arc())
    }

    /// Execute a command via SSH exec channel (does not show in terminal)
    /// Returns the command output as a string
    pub async fn exec_command(&self, command: &str) -> Result<String> {
        let ssh_guard = self.ssh_client.lock().await;
        if let Some(ref client) = *ssh_guard {
            let output = client.exec_command(command).await?;
            drop(ssh_guard);
            self.mark_activity().await;
            Ok(output)
        } else {
            Err(anyhow::anyhow!("SSH client not connected"))
        }
    }

    /// Open an SFTP subsystem session on a new SSH channel.
    /// Returns an SftpSession for performing file operations via the SFTP protocol.
    pub async fn open_sftp_session(&self) -> Result<russh_sftp::client::SftpSession> {
        let ssh_guard = self.ssh_client.lock().await;
        if let Some(ref client) = *ssh_guard {
            client.open_sftp_session().await
        } else {
            Err(anyhow::anyhow!("SSH client not connected"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_output_preserves_initial_chunks_for_late_subscribers() {
        let runtime = tokio::runtime::Runtime::new().expect("create tokio runtime");

        runtime.block_on(async {
            let (input_tx, _input_rx) = mpsc::channel(1);
            let (output_tx, _output_rx) = broadcast::channel(8);
            let session = Session::new(
                "server-1".to_string(),
                "test-server".to_string(),
                input_tx,
                output_tx,
            );

            session.publish_output(b"root@test".to_vec()).await;
            session.publish_output(b":~# ".to_vec()).await;

            let replay = session.replay_output().await;
            assert_eq!(replay, vec![b"root@test".to_vec(), b":~# ".to_vec()]);

            let mut receiver = session.subscribe();
            session.publish_output(b"pwd\r\n".to_vec()).await;

            assert_eq!(receiver.recv().await.expect("broadcast output"), b"pwd\r\n");
        });
    }

    #[test]
    fn session_is_reapable_only_when_idle_and_detached() {
        let runtime = tokio::runtime::Runtime::new().expect("create tokio runtime");

        runtime.block_on(async {
            let (input_tx, _input_rx) = mpsc::channel(1);
            let (output_tx, _output_rx) = broadcast::channel(8);
            let session = Session::new(
                "server-1".to_string(),
                "test-server".to_string(),
                input_tx,
                output_tx,
            );

            session.attach().await;
            assert!(
                !session.should_reap(Duration::from_secs(0)).await,
                "attached sessions should not be reaped"
            );

            session.detach().await;
            assert!(
                session.should_reap(Duration::from_secs(0)).await,
                "detached idle sessions should be reapable"
            );
        });
    }
}
