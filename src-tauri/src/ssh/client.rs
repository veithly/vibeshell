use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use log::{debug, error, info, warn};
use russh::*;
use russh_keys::*;
use russh_sftp::client::SftpSession;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};

/// Captured server key information from the SSH handshake
#[derive(Debug, Clone)]
pub struct ServerKeyInfo {
    /// SHA256 fingerprint of the server's public key
    pub fingerprint: String,
    /// The algorithm used (e.g., "ssh-ed25519", "ssh-rsa")
    pub algorithm: String,
}

#[derive(Clone)]
pub struct SshClient {
    session: Arc<Mutex<Option<client::Handle<ClientHandler>>>>,
    channel: Arc<Mutex<Option<Channel<client::Msg>>>>,
    output_tx: mpsc::Sender<Vec<u8>>,
    /// The channel ID of the shell channel - only data from this channel should go to the terminal
    shell_channel_id: Arc<Mutex<Option<ChannelId>>>,
    /// Captured server key from the most recent connection attempt
    server_key: Arc<Mutex<Option<ServerKeyInfo>>>,
}

pub struct ClientHandler {
    output_tx: mpsc::Sender<Vec<u8>>,
    /// Reference to the shell channel ID - only forward data from this channel
    shell_channel_id: Arc<Mutex<Option<ChannelId>>>,
    /// Storage for the captured server key
    server_key: Arc<Mutex<Option<ServerKeyInfo>>>,
}

/// PTY configuration for terminal sessions
#[derive(Clone)]
pub struct PtyConfig {
    pub term: String,
    pub cols: u32,
    pub rows: u32,
    pub pix_width: u32,
    pub pix_height: u32,
}

#[async_trait]
impl client::Handler for ClientHandler {
    type Error = anyhow::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &key::PublicKey,
    ) -> Result<bool, Self::Error> {
        // Extract and store the server's fingerprint for later verification
        let (fingerprint, algorithm) = crate::ssh::extract_fingerprint_from_key(server_public_key);

        info!(
            "[SSH] Server key received - Algorithm: {}, Fingerprint: {}",
            algorithm, fingerprint
        );

        // Store the captured key info
        let key_info = ServerKeyInfo {
            fingerprint,
            algorithm,
        };

        let mut server_key_guard = self.server_key.lock().await;
        *server_key_guard = Some(key_info);

        // Always accept the key during handshake - verification happens after
        // The caller is responsible for checking the fingerprint before proceeding
        Ok(true)
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        // Only forward data from the shell channel to the terminal
        // Data from exec channels (used for SFTP) should NOT go to the terminal
        let shell_id = self.shell_channel_id.lock().await;
        if let Some(shell_channel_id) = *shell_id {
            if channel == shell_channel_id {
                let _ = self.output_tx.send(data.to_vec()).await;
            } else {
                debug!(
                    "[SSH] Ignoring data from non-shell channel {:?} ({} bytes)",
                    channel,
                    data.len()
                );
            }
        } else {
            // Shell not opened yet - this shouldn't happen but log it
            debug!(
                "[SSH] Received data before shell opened, channel {:?}",
                channel
            );
        }
        Ok(())
    }
}

impl Default for PtyConfig {
    fn default() -> Self {
        Self {
            term: "xterm-256color".to_string(),
            cols: 80,
            rows: 24,
            pix_width: 0,
            pix_height: 0,
        }
    }
}

impl SshClient {
    const EXEC_COMMAND_TIMEOUT: Duration = Duration::from_secs(300);
    const EXEC_CLOSE_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);
    const EXEC_WAIT_TICK: Duration = Duration::from_millis(250);

    pub fn new(output_tx: mpsc::Sender<Vec<u8>>) -> Self {
        Self {
            session: Arc::new(Mutex::new(None)),
            channel: Arc::new(Mutex::new(None)),
            output_tx,
            shell_channel_id: Arc::new(Mutex::new(None)),
            server_key: Arc::new(Mutex::new(None)),
        }
    }

    /// Get the server key info captured during the last connection attempt
    /// Returns None if no connection has been made or the key wasn't captured
    pub async fn get_server_key(&self) -> Option<ServerKeyInfo> {
        let guard = self.server_key.lock().await;
        guard.clone()
    }

    /// Clear the captured server key
    pub async fn clear_server_key(&self) {
        let mut guard = self.server_key.lock().await;
        *guard = None;
    }

    /// Get a clone of the channel Arc for sharing with other tasks
    pub fn channel_handle(&self) -> Arc<Mutex<Option<Channel<client::Msg>>>> {
        self.channel.clone()
    }

    /// Get the SSH session handle Arc for tunnel/forwarding use
    pub fn session_arc(&self) -> Arc<Mutex<Option<client::Handle<ClientHandler>>>> {
        self.session.clone()
    }

    pub async fn connect_password(
        &mut self,
        host: &str,
        port: u16,
        username: &str,
        password: &str,
    ) -> Result<()> {
        info!(
            "[SSH] Starting password authentication to {}:{}",
            host, port
        );
        debug!(
            "[SSH] Username: {}, Password length: {}",
            username,
            password.len()
        );

        // Clear any previously captured server key
        self.clear_server_key().await;

        // Configure SSH client with proper timeout and keepalive settings
        // Without these, connections may be dropped during the handshake phase
        let config = Arc::new(client::Config {
            inactivity_timeout: Some(std::time::Duration::from_secs(60)),
            keepalive_interval: Some(std::time::Duration::from_secs(10)),
            keepalive_max: 5,
            ..Default::default()
        });
        debug!("[SSH] Config: inactivity_timeout=60s, keepalive_interval=10s, keepalive_max=5");

        let handler = ClientHandler {
            output_tx: self.output_tx.clone(),
            shell_channel_id: self.shell_channel_id.clone(),
            server_key: self.server_key.clone(),
        };

        let tcp_timeout = tokio::time::Duration::from_secs(300);
        info!(
            "[SSH] Attempting TCP connection to {}:{} (timeout: {}s)...",
            host,
            port,
            tcp_timeout.as_secs()
        );
        let mut session =
            tokio::time::timeout(tcp_timeout, client::connect(config, (host, port), handler))
                .await
                .map_err(|_| {
                    anyhow!(
                "TCP connection to {}:{} timed out after {}s (check network/Tailscale/VPN status)",
                host,
                port,
                tcp_timeout.as_secs()
            )
                })?
                .with_context(|| format!("Failed to connect to {}:{}", host, port))?;

        info!("[SSH] TCP connection established, starting password authentication...");
        let auth_result = session
            .authenticate_password(username, password)
            .await
            .with_context(|| format!("Password authentication failed for user '{}'", username))?;

        if !auth_result {
            error!(
                "[SSH] Authentication rejected by server for user '{}'",
                username
            );
            return Err(anyhow!(
                "Authentication failed: server rejected credentials for user '{}'",
                username
            ));
        }

        info!(
            "[SSH] Password authentication successful for user '{}'",
            username
        );
        {
            let mut session_guard = self.session.lock().await;
            *session_guard = Some(session);
        }
        Ok(())
    }

    pub async fn connect_key(
        &mut self,
        host: &str,
        port: u16,
        username: &str,
        private_key: &str,
        passphrase: Option<&str>,
    ) -> Result<()> {
        info!("[SSH] Starting key authentication to {}:{}", host, port);
        debug!(
            "[SSH] Username: {}, Key length: {}, Has passphrase: {}",
            username,
            private_key.len(),
            passphrase.is_some()
        );

        // Clear any previously captured server key
        self.clear_server_key().await;

        // Configure SSH client with proper timeout and keepalive settings
        // Without these, connections may be dropped during the handshake phase
        let config = Arc::new(client::Config {
            inactivity_timeout: Some(std::time::Duration::from_secs(60)),
            keepalive_interval: Some(std::time::Duration::from_secs(10)),
            keepalive_max: 5,
            ..Default::default()
        });
        debug!("[SSH] Config: inactivity_timeout=60s, keepalive_interval=10s, keepalive_max=5");

        let handler = ClientHandler {
            output_tx: self.output_tx.clone(),
            shell_channel_id: self.shell_channel_id.clone(),
            server_key: self.server_key.clone(),
        };

        let tcp_timeout = tokio::time::Duration::from_secs(300);
        info!(
            "[SSH] Attempting TCP connection to {}:{} (timeout: {}s)...",
            host,
            port,
            tcp_timeout.as_secs()
        );
        let mut session =
            tokio::time::timeout(tcp_timeout, client::connect(config, (host, port), handler))
                .await
                .map_err(|_| {
                    anyhow!(
                "TCP connection to {}:{} timed out after {}s (check network/Tailscale/VPN status)",
                host,
                port,
                tcp_timeout.as_secs()
            )
                })?
                .with_context(|| format!("Failed to connect to {}:{}", host, port))?;

        info!("[SSH] TCP connection established, parsing private key...");
        let key_pair = if let Some(pass) = passphrase {
            decode_secret_key(private_key, Some(pass))
                .with_context(|| "Failed to decode private key with passphrase")?
        } else {
            decode_secret_key(private_key, None)
                .with_context(|| "Failed to decode private key (no passphrase)")?
        };
        info!("[SSH] Private key parsed successfully");

        info!(
            "[SSH] Starting public key authentication for user '{}'...",
            username
        );
        let auth_result = session
            .authenticate_publickey(username, Arc::new(key_pair))
            .await
            .with_context(|| format!("Public key authentication failed for user '{}'", username))?;

        if !auth_result {
            error!(
                "[SSH] Authentication rejected by server for user '{}'",
                username
            );
            return Err(anyhow!(
                "Key authentication failed: server rejected credentials for user '{}'",
                username
            ));
        }

        info!(
            "[SSH] Key authentication successful for user '{}'",
            username
        );
        {
            let mut session_guard = self.session.lock().await;
            *session_guard = Some(session);
        }
        Ok(())
    }

    pub async fn disconnect(&mut self) -> Result<()> {
        info!("[SSH] Disconnecting...");
        // Clear the shell channel ID first
        {
            let mut shell_id_guard = self.shell_channel_id.lock().await;
            *shell_id_guard = None;
        }

        // Close the channel
        {
            let mut channel_guard = self.channel.lock().await;
            if let Some(channel) = channel_guard.take() {
                debug!("[SSH] Closing channel with EOF");
                let _ = channel.eof().await;
            }
        }

        {
            let mut session_guard = self.session.lock().await;
            if let Some(session) = session_guard.take() {
                debug!("[SSH] Sending disconnect to server");
                session
                    .disconnect(Disconnect::ByApplication, "", "en")
                    .await?;
            }
        }
        info!("[SSH] Disconnected successfully");
        Ok(())
    }

    pub async fn is_connected(&self) -> bool {
        let session_guard = self.session.lock().await;
        session_guard.is_some()
    }

    /// Open a shell channel with PTY
    pub async fn open_shell(&mut self, pty_config: Option<PtyConfig>) -> Result<()> {
        info!("[SSH] Opening shell channel...");
        let session_guard = self.session.lock().await;
        let session = session_guard.as_ref().ok_or_else(|| {
            error!("[SSH] Cannot open shell: not connected");
            anyhow!("Not connected")
        })?;

        info!("[SSH] Opening session channel...");
        let channel = session
            .channel_open_session()
            .await
            .with_context(|| "Failed to open session channel")?;

        // Store the shell channel ID BEFORE any data might come in
        // This ensures the handler knows which channel is the shell
        let channel_id = channel.id();
        {
            let mut shell_id_guard = self.shell_channel_id.lock().await;
            *shell_id_guard = Some(channel_id);
            info!("[SSH] Shell channel ID set to {:?}", channel_id);
        }

        let pty = pty_config.unwrap_or_default();
        info!(
            "[SSH] Requesting PTY (term={}, cols={}, rows={})",
            pty.term, pty.cols, pty.rows
        );

        // Request a pseudo-terminal
        channel
            .request_pty(
                false, // want_reply
                &pty.term,
                pty.cols,
                pty.rows,
                pty.pix_width,
                pty.pix_height,
                &[], // No special terminal modes
            )
            .await
            .with_context(|| "Failed to request PTY")?;

        info!("[SSH] Requesting shell...");
        // Request a shell
        channel
            .request_shell(false)
            .await
            .with_context(|| "Failed to request shell")?;

        // Store the channel
        let mut channel_guard = self.channel.lock().await;
        *channel_guard = Some(channel);

        info!("[SSH] Shell opened successfully");
        Ok(())
    }

    /// Send data to the SSH shell stdin
    pub async fn send_data(&self, data: &[u8]) -> Result<()> {
        debug!("[SSH] Sending {} bytes to shell", data.len());
        let channel_guard = self.channel.lock().await;
        let channel = channel_guard.as_ref().ok_or_else(|| {
            warn!("[SSH] Cannot send data: shell not opened");
            anyhow!("Shell not opened")
        })?;

        channel
            .data(data)
            .await
            .with_context(|| "Failed to send data to shell")?;
        Ok(())
    }

    /// Resize the PTY window
    pub async fn resize_pty(&self, cols: u32, rows: u32) -> Result<()> {
        debug!("[SSH] Resizing PTY to {}x{}", cols, rows);
        let channel_guard = self.channel.lock().await;
        let channel = channel_guard.as_ref().ok_or_else(|| {
            warn!("[SSH] Cannot resize: shell not opened");
            anyhow!("Shell not opened")
        })?;

        channel
            .window_change(cols, rows, 0, 0)
            .await
            .with_context(|| "Failed to resize PTY")?;
        Ok(())
    }

    /// Open an SFTP subsystem session on a new channel.
    /// Returns an `SftpSession` that can be used for file operations.
    pub async fn open_sftp_session(&self) -> Result<SftpSession> {
        info!("[SSH] Opening SFTP subsystem channel...");

        let channel = {
            let session_guard = self.session.lock().await;
            let session = session_guard.as_ref().ok_or_else(|| {
                error!("[SSH] Cannot open SFTP: not connected");
                anyhow!("Not connected")
            })?;

            session
                .channel_open_session()
                .await
                .with_context(|| "Failed to open SFTP channel")?
        };

        channel
            .request_subsystem(true, "sftp")
            .await
            .with_context(|| "Failed to request SFTP subsystem")?;

        info!("[SSH] SFTP subsystem requested, initializing session...");

        let sftp = SftpSession::new(channel.into_stream())
            .await
            .map_err(|e| anyhow!("Failed to initialize SFTP session: {}", e))?;

        info!("[SSH] SFTP session initialized successfully");
        Ok(sftp)
    }

    /// Execute a command on a separate exec channel and capture output.
    /// This does NOT use the shell channel, so output won't appear in the terminal.
    pub async fn exec_command(&self, command: &str) -> Result<String> {
        self.exec_command_with_stdin(command, None).await
    }

    /// Like [`exec_command`] but optionally writes `stdin_data` to the channel's
    /// stdin after starting the command, then signals EOF. Used by elevated
    /// plugin actions to feed a sudo password to `sudo -S`.
    pub async fn exec_command_with_stdin(
        &self,
        command: &str,
        stdin_data: Option<&str>,
    ) -> Result<String> {
        debug!("[SSH] Executing command via exec channel: {}", command);

        // Open the channel while holding the russh handle lock, then release it.
        // The returned Channel owns its own sender/receiver, so command execution
        // must not block unrelated SFTP/tunnel/session operations from opening
        // their own channels.
        let mut channel = {
            let session_guard = self.session.lock().await;
            let session = session_guard.as_ref().ok_or_else(|| {
                error!("[SSH] Cannot exec: not connected");
                anyhow!("Not connected")
            })?;

            session
                .channel_open_session()
                .await
                .with_context(|| "Failed to open exec channel")?
        };

        // Execute the command (not a shell, just exec)
        if let Err(error) = channel.exec(true, command).await {
            let _ = channel.close().await;
            return Err(error).with_context(|| format!("Failed to exec command: {}", command));
        }

        // Feed stdin (e.g. a sudo password) and close our side so the remote
        // process can proceed. Failures here are non-fatal: the command still
        // runs, it just won't receive the password.
        if let Some(data) = stdin_data {
            if !data.is_empty() {
                let payload = format!("{data}\n");
                if let Err(error) = channel.data(payload.as_bytes()).await {
                    debug!("[SSH] Failed to write plugin stdin: {}", error);
                }
            }
            if let Err(error) = channel.eof().await {
                debug!("[SSH] Failed to signal EOF after stdin: {}", error);
            }
        }

        // Collect output
        let mut output = Vec::new();
        let start = tokio::time::Instant::now();
        let mut exit_status = None;
        let mut remote_closed = false;
        let mut timed_out = false;
        let mut last_message_at = start;
        let mut received_eof = false;

        loop {
            let elapsed = start.elapsed();
            if elapsed >= Self::EXEC_COMMAND_TIMEOUT {
                timed_out = true;
                warn!(
                    "[SSH] Command execution timed out after {:?}",
                    Self::EXEC_COMMAND_TIMEOUT
                );
                break;
            }

            let remaining = Self::EXEC_COMMAND_TIMEOUT.saturating_sub(elapsed);
            let wait_for = remaining.min(Self::EXEC_WAIT_TICK);

            match tokio::time::timeout(wait_for, channel.wait()).await {
                Ok(Some(msg)) => {
                    last_message_at = tokio::time::Instant::now();
                    match msg {
                        ChannelMsg::Data { data } => {
                            output.extend_from_slice(&data);
                        }
                        ChannelMsg::ExtendedData { data, .. } => {
                            // stderr - also capture it
                            output.extend_from_slice(&data);
                        }
                        ChannelMsg::Eof => {
                            debug!("[SSH] Received EOF from exec channel");
                            received_eof = true;
                            // Keep draining until Close/None so russh can retire the channel.
                        }
                        ChannelMsg::ExitStatus {
                            exit_status: status,
                        } => {
                            debug!("[SSH] Command exit status: {}", status);
                            // Continue to collect any remaining output and wait for Close.
                            exit_status = Some(status);
                        }
                        ChannelMsg::Close => {
                            debug!("[SSH] Exec channel closed");
                            remote_closed = true;
                            break;
                        }
                        _ => {}
                    }
                }
                Ok(None) => {
                    // Channel closed
                    remote_closed = true;
                    break;
                }
                Err(_) => {
                    if (exit_status.is_some() || received_eof)
                        && last_message_at.elapsed() >= Self::EXEC_CLOSE_DRAIN_TIMEOUT
                    {
                        debug!(
                            "[SSH] Exec channel did not close after completion; closing locally"
                        );
                        break;
                    }
                }
            }
        }

        // Always close channels we opened. Sending only EOF is not enough: on
        // reused OpenSSH sessions it can leave exec channels counted against
        // MaxSessions, causing later channel_open_session calls to fail.
        if !remote_closed {
            let _ = channel.eof().await;
            let _ = channel.close().await;
        }

        if timed_out {
            return Err(anyhow!(
                "Command timed out after {}s",
                Self::EXEC_COMMAND_TIMEOUT.as_secs()
            ));
        }

        let output_str = String::from_utf8_lossy(&output).to_string();
        debug!(
            "[SSH] Command output ({} bytes): {:?}",
            output_str.len(),
            if output_str.len() > 100 {
                &output_str[..100]
            } else {
                &output_str
            }
        );

        Ok(output_str)
    }
}
