use anyhow::{Result, anyhow};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use log::{info, warn, error, debug};

use crate::session::{Session, SessionInfo, SessionState};
use crate::ssh::{SshClient, PtyConfig};
use crate::storage::{Database, Server};

/// Credentials for SSH authentication
#[derive(Debug, Clone)]
pub enum SshCredential {
    Password(String),
    PrivateKey { key: String, passphrase: Option<String> },
}

pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<String, Arc<Session>>>>,
    database: Arc<Database>,
}

impl SessionManager {
    pub fn new(database: Arc<Database>) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            database,
        }
    }

    pub async fn list(&self) -> Vec<SessionInfo> {
        let sessions = self.sessions.read().await;
        let mut infos = Vec::new();
        for session in sessions.values() {
            infos.push(session.get_info().await);
        }
        infos
    }

    pub async fn get(&self, id: &str) -> Option<Arc<Session>> {
        let sessions = self.sessions.read().await;
        sessions.get(id).cloned()
    }

    pub async fn create(&self, server_id: &str) -> Result<Arc<Session>> {
        let server = self.database.server_get(server_id)?
            .ok_or_else(|| anyhow!("Server not found: {}", server_id))?;
        self.create_for_server(&server).await
    }

    pub async fn create_by_name(&self, server_name: &str) -> Result<Arc<Session>> {
        let server = self.database.server_get_by_name(server_name)?
            .ok_or_else(|| anyhow!("Server not found: {}", server_name))?;
        self.create_for_server(&server).await
    }

    async fn create_for_server(&self, server: &Server) -> Result<Arc<Session>> {
        // For backward compatibility, create a session in connecting state
        // The actual SSH connection should be done via create_with_credentials
        let (input_tx, _input_rx) = tokio::sync::mpsc::channel(256);
        let (output_tx, _) = tokio::sync::broadcast::channel(256);

        let session = Arc::new(Session::new(
            server.id.clone(),
            server.name.clone(),
            input_tx,
            output_tx,
        ));

        // Session stays in Connecting state until connect_session is called
        let mut sessions = self.sessions.write().await;
        sessions.insert(session.id.clone(), session.clone());

        Ok(session)
    }

    /// Create a session and connect with provided credentials
    /// Supports jump host (ProxyJump) connections and post-login commands
    pub async fn create_with_credentials(
        &self,
        server_name: &str,
        credential: SshCredential,
        pty_config: Option<PtyConfig>,
    ) -> Result<Arc<Session>> {
        info!("[SessionManager] Creating session for server '{}'", server_name);

        let server = self.database.server_get_by_name(server_name)?
            .ok_or_else(|| {
                error!("[SessionManager] Server not found: {}", server_name);
                anyhow!("Server not found: {}", server_name)
            })?;

        info!("[SessionManager] Found server: {}@{}:{}", server.username, server.host, server.port);

        // Create channels for input/output
        let (input_tx, mut input_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(256);
        let (output_tx, _) = tokio::sync::broadcast::channel::<Vec<u8>>(256);

        // Create a channel for SSH output that will be bridged to broadcast
        let (ssh_output_tx, mut ssh_output_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(256);

        // Create the session
        let session = Arc::new(Session::new(
            server.id.clone(),
            server.name.clone(),
            input_tx,
            output_tx.clone(),
        ));

        info!("[SessionManager] Session created with ID: {}", session.id);

        // === Jump Host Support ===
        // If server has a jump_host_id, we connect through the jump host first
        let _jump_bridge_handle: Option<tokio::task::JoinHandle<()>> = if let Some(ref jump_id) = server.jump_host_id {
            info!("[SessionManager] Server has jump host configured: {}", jump_id);

            let jump_server = self.database.server_get(jump_id)?
                .ok_or_else(|| {
                    error!("[SessionManager] Jump host server not found: {}", jump_id);
                    anyhow!("Jump host server not found: {}", jump_id)
                })?;

            info!("[SessionManager] Jump host: {}@{}:{}", jump_server.username, jump_server.host, jump_server.port);

            // Get saved credentials for jump host
            let jump_cred = self.database.credential_get(&jump_server.name)?
                .ok_or_else(|| {
                    error!("[SessionManager] No saved credentials for jump host '{}'", jump_server.name);
                    anyhow!("No saved credentials for jump host '{}'. Please save credentials first.", jump_server.name)
                })?;

            // Create a separate SSH client for the jump host (with a dummy output channel)
            let (jump_output_tx, _jump_output_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(16);
            let mut jump_ssh = SshClient::new(jump_output_tx);

            // Connect to jump host
            match jump_cred.auth_type.as_str() {
                "password" => {
                    info!("[SessionManager] Connecting to jump host with password...");
                    jump_ssh.connect_password(
                        &jump_server.host,
                        jump_server.port,
                        &jump_server.username,
                        &jump_cred.credential,
                    ).await?;
                }
                "key" | "key_with_passphrase" => {
                    info!("[SessionManager] Connecting to jump host with key...");
                    jump_ssh.connect_key(
                        &jump_server.host,
                        jump_server.port,
                        &jump_server.username,
                        &jump_cred.credential,
                        jump_cred.passphrase.as_deref(),
                    ).await?;
                }
                _ => {
                    return Err(anyhow!("Unknown auth type for jump host: {}", jump_cred.auth_type));
                }
            }

            info!("[SessionManager] Connected to jump host, opening tunnel to target...");

            // Open a direct-tcpip channel from jump host to the target server
            let jump_handle_arc = jump_ssh.session_arc();
            let forward_channel = {
                let handle_guard = jump_handle_arc.lock().await;
                let handle = handle_guard.as_ref()
                    .ok_or_else(|| anyhow!("Jump host SSH session not available"))?;
                handle
                    .channel_open_direct_tcpip(&server.host, server.port as u32, "127.0.0.1", 0)
                    .await
                    .map_err(|e| anyhow!("Failed to open tunnel through jump host: {}", e))?
            };

            info!("[SessionManager] Tunnel through jump host established");

            // Create a local TCP listener bridge
            // The target SSH client will connect to this local port which bridges through the jump host
            let bridge_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
            let bridge_port = bridge_listener.local_addr()?.port();
            info!("[SessionManager] Bridge listener on 127.0.0.1:{}", bridge_port);

            // Spawn bridge task
            let bridge_handle = tokio::spawn(async move {
                match bridge_listener.accept().await {
                    Ok((tcp_stream, _)) => {
                        // Use into_stream() for bidirectional channel I/O
                        let channel_stream = forward_channel.into_stream();
                        let (mut ch_reader, mut ch_writer) = tokio::io::split(channel_stream);
                        let (mut tcp_reader, mut tcp_writer) = tokio::io::split(tcp_stream);

                        let (done_tx, mut done_rx) = tokio::sync::mpsc::channel::<()>(2);

                        // TCP -> SSH channel (to target through jump host)
                        let done1 = done_tx.clone();
                        let t2s = tokio::spawn(async move {
                            use tokio::io::{AsyncReadExt, AsyncWriteExt};
                            let mut buf = vec![0u8; 32768];
                            loop {
                                match tcp_reader.read(&mut buf).await {
                                    Ok(0) => break,
                                    Ok(n) => {
                                        if (ch_writer.write_all(&buf[..n]).await).is_err() { break; }
                                    }
                                    Err(_) => break,
                                }
                            }
                            let _ = ch_writer.shutdown().await;
                            let _ = done1.send(()).await;
                        });

                        // SSH channel -> TCP (from target through jump host)
                        let done2 = done_tx;
                        let s2t = tokio::spawn(async move {
                            use tokio::io::{AsyncReadExt, AsyncWriteExt};
                            let mut buf = vec![0u8; 32768];
                            loop {
                                match ch_reader.read(&mut buf).await {
                                    Ok(0) => break,
                                    Ok(n) => {
                                        if (tcp_writer.write_all(&buf[..n]).await).is_err() { break; }
                                    }
                                    Err(_) => break,
                                }
                            }
                            let _ = tcp_writer.shutdown().await;
                            let _ = done2.send(()).await;
                        });

                        done_rx.recv().await;
                        t2s.abort();
                        s2t.abort();
                    }
                    Err(e) => {
                        error!("[SessionManager] Bridge accept error: {}", e);
                    }
                }
                // Keep jump_ssh alive as long as this task runs
                drop(jump_ssh);
            });

            // Now connect the target SSH through our local bridge
            let mut ssh_client = SshClient::new(ssh_output_tx);
            info!("[SessionManager] Connecting to target through bridge at 127.0.0.1:{}...", bridge_port);

            match credential {
                SshCredential::Password(password) => {
                    ssh_client.connect_password(
                        "127.0.0.1",
                        bridge_port,
                        &server.username,
                        &password,
                    ).await?;
                }
                SshCredential::PrivateKey { key, passphrase } => {
                    ssh_client.connect_key(
                        "127.0.0.1",
                        bridge_port,
                        &server.username,
                        &key,
                        passphrase.as_deref(),
                    ).await?;
                }
            }

            info!("[SessionManager] Target SSH connected via jump host, opening shell...");
            ssh_client.open_shell(pty_config).await?;

            session.set_ssh_client(ssh_client).await;
            session.set_state(SessionState::Connected).await;

            Some(bridge_handle)
        } else {
            // === Direct Connection (no jump host) ===
            let mut ssh_client = SshClient::new(ssh_output_tx);

            info!("[SessionManager] Connecting SSH client...");
            match &credential {
                SshCredential::Password(_) => {
                    info!("[SessionManager] Using password authentication");
                }
                SshCredential::PrivateKey { passphrase, .. } => {
                    info!("[SessionManager] Using key authentication (passphrase: {})",
                          if passphrase.is_some() { "yes" } else { "no" });
                }
            }

            match credential {
                SshCredential::Password(password) => {
                    ssh_client.connect_password(
                        &server.host,
                        server.port,
                        &server.username,
                        &password,
                    ).await?;
                }
                SshCredential::PrivateKey { key, passphrase } => {
                    ssh_client.connect_key(
                        &server.host,
                        server.port,
                        &server.username,
                        &key,
                        passphrase.as_deref(),
                    ).await?;
                }
            }

            info!("[SessionManager] SSH connected, opening shell...");
            ssh_client.open_shell(pty_config).await?;

            session.set_ssh_client(ssh_client).await;
            session.set_state(SessionState::Connected).await;

            None
        };

        info!("[SessionManager] Shell opened successfully");

        // Spawn task to bridge SSH output to broadcast channel
        let output_tx_clone = output_tx.clone();
        let session_id_for_output = session.id.clone();
        tokio::spawn(async move {
            debug!("[SessionManager] Output bridge task started for session {}", session_id_for_output);
            while let Some(data) = ssh_output_rx.recv().await {
                // Ignore send errors (no receivers)
                let _ = output_tx_clone.send(data);
            }
            debug!("[SessionManager] Output bridge task ended for session {}", session_id_for_output);
        });

        // Spawn task to bridge input channel to SSH stdin
        let session_clone = session.clone();
        let session_id_for_input = session.id.clone();
        tokio::spawn(async move {
            debug!("[SessionManager] Input bridge task started for session {}", session_id_for_input);
            while let Some(data) = input_rx.recv().await {
                if let Err(e) = session_clone.write_to_ssh(&data).await {
                    error!("[SessionManager] Error writing to SSH for session {}: {}", session_id_for_input, e);
                    break;
                }
            }
            debug!("[SessionManager] Input bridge task ended for session {}", session_id_for_input);
        });

        // === Post-login Command ===
        if let Some(ref cmd) = server.post_login_command {
            if !cmd.trim().is_empty() {
                let session_for_cmd = session.clone();
                let cmd_str = cmd.clone();
                let sid = session.id.clone();
                tokio::spawn(async move {
                    // Small delay to let shell initialize
                    tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                    let cmd_with_newline = format!("{}\n", cmd_str);
                    if let Err(e) = session_for_cmd.write_to_ssh(cmd_with_newline.as_bytes()).await {
                        warn!("[SessionManager] Failed to send post-login command for session {}: {}", sid, e);
                    } else {
                        info!("[SessionManager] Sent post-login command for session {}: {}", sid, cmd_str);
                    }
                });
            }
        }

        // Store the session
        let mut sessions = self.sessions.write().await;
        sessions.insert(session.id.clone(), session.clone());

        info!("[SessionManager] Session {} ready and connected", session.id);
        Ok(session)
    }

    /// Connect an existing session with credentials
    pub async fn connect_session(
        &self,
        session_id: &str,
        credential: SshCredential,
        pty_config: Option<PtyConfig>,
    ) -> Result<()> {
        info!("[SessionManager] Connecting existing session {}", session_id);

        let session = {
            let sessions = self.sessions.read().await;
            sessions.get(session_id).cloned()
                .ok_or_else(|| {
                    error!("[SessionManager] Session not found: {}", session_id);
                    anyhow!("Session not found: {}", session_id)
                })?
        };

        // Get server info
        let server = self.database.server_get(&session.server_id)?
            .ok_or_else(|| {
                error!("[SessionManager] Server not found for session {}", session_id);
                anyhow!("Server not found for session")
            })?;

        info!("[SessionManager] Connecting to {}@{}:{}", server.username, server.host, server.port);

        // Create channel for SSH output
        let (ssh_output_tx, mut ssh_output_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(256);

        // Create and connect SSH client
        let mut ssh_client = SshClient::new(ssh_output_tx);

        info!("[SessionManager] Starting SSH connection...");
        match &credential {
            SshCredential::Password(_) => {
                info!("[SessionManager] Using password authentication");
            }
            SshCredential::PrivateKey { passphrase, .. } => {
                info!("[SessionManager] Using key authentication (passphrase: {})",
                      if passphrase.is_some() { "yes" } else { "no" });
            }
        }

        match credential {
            SshCredential::Password(password) => {
                ssh_client.connect_password(
                    &server.host,
                    server.port,
                    &server.username,
                    &password,
                ).await?;
            }
            SshCredential::PrivateKey { key, passphrase } => {
                ssh_client.connect_key(
                    &server.host,
                    server.port,
                    &server.username,
                    &key,
                    passphrase.as_deref(),
                ).await?;
            }
        }

        info!("[SessionManager] SSH connected, opening shell...");

        // Open shell with PTY
        ssh_client.open_shell(pty_config).await?;

        info!("[SessionManager] Shell opened successfully for session {}", session_id);

        // Store the SSH client in the session
        session.set_ssh_client(ssh_client).await;
        session.set_state(SessionState::Connected).await;

        // Bridge SSH output to session broadcast
        let output_tx = session.output_sender();
        let session_id_clone = session_id.to_string();
        tokio::spawn(async move {
            debug!("[SessionManager] Output bridge task started for session {}", session_id_clone);
            while let Some(data) = ssh_output_rx.recv().await {
                let _ = output_tx.send(data);
            }
            debug!("[SessionManager] Output bridge task ended for session {}", session_id_clone);
        });

        info!("[SessionManager] Session {} connected successfully", session_id);
        Ok(())
    }

    pub async fn kill(&self, id: &str) -> Result<()> {
        info!("[SessionManager] Killing session {}", id);
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.remove(id) {
            // Disconnect SSH gracefully
            if let Err(e) = session.disconnect().await {
                error!("[SessionManager] Error disconnecting session {}: {}", id, e);
            } else {
                info!("[SessionManager] Session {} disconnected successfully", id);
            }
        } else {
            warn!("[SessionManager] Session {} not found for kill", id);
        }
        Ok(())
    }

    pub async fn kill_all(&self) -> Result<()> {
        info!("[SessionManager] Killing all sessions");
        let mut sessions = self.sessions.write().await;
        let count = sessions.len();
        for (id, session) in sessions.iter() {
            if let Err(e) = session.disconnect().await {
                error!("[SessionManager] Error disconnecting session {}: {}", id, e);
            }
        }
        sessions.clear();
        info!("[SessionManager] Killed {} sessions", count);
        Ok(())
    }
}
