use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock, Mutex};
use uuid::Uuid;

use crate::ssh::{SshClient, ClientHandler};

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

    // Track connected clients
    client_count: Arc<RwLock<usize>>,
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
            client_count: Arc::new(RwLock::new(0)),
        }
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

    pub async fn send_input(&self, data: Vec<u8>) -> Result<()> {
        self.input_tx.send(data).await?;
        Ok(())
    }

    pub async fn attach(&self) {
        let mut count = self.client_count.write().await;
        *count += 1;
    }

    pub async fn detach(&self) {
        let mut count = self.client_count.write().await;
        if *count > 0 {
            *count -= 1;
        }
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
    pub async fn get_ssh_handle_arc(&self) -> Option<Arc<tokio::sync::Mutex<Option<russh::client::Handle<ClientHandler>>>>> {
        let ssh_guard = self.ssh_client.lock().await;
        (*ssh_guard).as_ref().map(|client| client.session_arc())
    }

    /// Execute a command via SSH exec channel (does not show in terminal)
    /// Returns the command output as a string
    pub async fn exec_command(&self, command: &str) -> Result<String> {
        let ssh_guard = self.ssh_client.lock().await;
        if let Some(ref client) = *ssh_guard {
            client.exec_command(command).await
        } else {
            Err(anyhow::anyhow!("SSH client not connected"))
        }
    }
}
