use anyhow::{anyhow, Context, Result};
use log::{debug, error, info, warn};
use russh::keys::{decode_secret_key, key::PrivateKeyWithHashAlg, HashAlg, PublicKeyOrCertificate};
use russh::*;
use russh_sftp::client::SftpSession;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};

use super::fingerprint::{evaluate_host_key, HostKeyCheck, HostKeyRejection};

/// Captured server key information from the SSH handshake
#[derive(Debug, Clone)]
pub struct ServerKeyInfo {
    /// SHA256 fingerprint of the server's public key
    pub fingerprint: String,
    /// The algorithm used (e.g., "ssh-ed25519", "ssh-rsa")
    pub algorithm: String,
}

/// A missing exit status stays unknown instead of being reported as success.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandResult {
    pub output: String,
    pub exit_code: i32,
}

/// TCP connect timeout for regular (authenticated) connections.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(300);
/// TCP connect timeout for the handshake-only host-key probe.
const PROBE_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Default)]
struct ShellReader(StdMutex<Option<tokio::task::JoinHandle<()>>>);
impl ShellReader {
    fn abort(&self) {
        if let Ok(mut task) = self.0.lock() {
            if let Some(task) = task.take() {
                task.abort();
            }
        }
    }
}
impl Drop for ShellReader {
    fn drop(&mut self) {
        self.abort();
    }
}

#[derive(Clone)]
pub struct SshClient {
    shell_reader: Arc<ShellReader>,
    remote_forwards: crate::tunnel::remote_forward::RemoteForwardRegistry,
    session: Arc<Mutex<Option<client::Handle<ClientHandler>>>>,
    channel: Arc<Mutex<Option<ChannelWriteHalf<client::Msg>>>>,
    output_tx: mpsc::Sender<Vec<u8>>,
    /// The channel ID of the shell channel - only data from this channel should go to the terminal
    shell_channel_id: Arc<Mutex<Option<ChannelId>>>,
    /// Captured server key from the most recent connection attempt
    server_key: Arc<Mutex<Option<ServerKeyInfo>>>,
    /// Host-key verification policy (TOFU). None = fail closed.
    host_key_check: Option<HostKeyCheck>,
    /// Rejection recorded by `check_server_key` when the presented host key
    /// was not trusted. Shared with the handler so the connect wrapper can
    /// convert the handshake abort into a typed, machine-parseable error.
    /// Reset at the start of every connect attempt.
    host_key_rejection: Arc<StdMutex<Option<HostKeyRejection>>>,
}

pub struct ClientHandler {
    remote_forwards: crate::tunnel::remote_forward::RemoteForwardRegistry,
    /// Storage for the captured server key
    server_key: Arc<Mutex<Option<ServerKeyInfo>>>,
    /// Host-key verification policy; when absent the handshake fails closed.
    host_key_check: Option<HostKeyCheck>,
    /// Shared slot where host-key rejections are recorded before aborting.
    host_key_rejection: Arc<StdMutex<Option<HostKeyRejection>>>,
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

impl client::Handler for ClientHandler {
    type Error = anyhow::Error;

    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: Channel<client::Msg>,
        connected_address: &str,
        connected_port: u32,
        _originator_address: &str,
        _originator_port: u32,
        reply: client::ChannelOpenHandle,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        let sender = self.remote_forwards.lock().ok().and_then(|routes| {
            routes
                .get(&(connected_address.to_string(), connected_port))
                .cloned()
        });
        if let Some(permit) = sender.and_then(|sender| sender.try_reserve_owned().ok()) {
            reply.accept().await;
            permit.send(channel);
        } else {
            reply
                .reject(ChannelOpenFailure::AdministrativelyProhibited)
                .await;
        }
        Ok(())
    }

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        // Extract and store the server's fingerprint for later verification
        let (fingerprint, algorithm) = crate::ssh::extract_fingerprint_from_key(server_public_key);

        info!(
            "[SSH] Server key received - Algorithm: {}, Fingerprint: {}",
            algorithm, fingerprint
        );

        // Store the captured key info (also used by the handshake-only probe).
        let key_info = ServerKeyInfo {
            fingerprint: fingerprint.clone(),
            algorithm: algorithm.clone(),
        };

        let mut server_key_guard = self.server_key.lock().await;
        *server_key_guard = Some(key_info);
        drop(server_key_guard);

        // TOFU enforcement. This runs during the SSH handshake, BEFORE any
        // credentials are transmitted, so a rejection can never leak the
        // password/key to a machine-in-the-middle. Returning `false` aborts
        // the handshake; the structured reason is recorded in the shared
        // rejection slot and surfaced by the connect wrapper.
        let Some(check) = &self.host_key_check else {
            warn!("[SSH] No host-key verifier configured; refusing host key (fail closed)");
            record_host_key_rejection(&self.host_key_rejection, HostKeyRejection::Unconfigured);
            return Ok(false);
        };

        // The GUI and daemon may be separate processes. Re-read persisted
        // approvals/revocations and fail closed if the trust store is damaged.
        check.refresh()?;
        let verdict = evaluate_host_key(&check.verify(&fingerprint, &algorithm));
        match verdict {
            Ok(()) => {
                debug!(
                    "[SSH] Host key trusted for {}:{}",
                    check.host(),
                    check.port()
                );
                Ok(true)
            }
            Err(rejection) => {
                warn!(
                    "[SSH] Host key REJECTED for {}:{} - aborting handshake before authentication",
                    check.host(),
                    check.port()
                );
                record_host_key_rejection(&self.host_key_rejection, rejection);
                Ok(false)
            }
        }
    }

    // PTY output is consumed by its channel reader rather than this global
    // callback, so terminal backpressure cannot block exec/SFTP callbacks.
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

/// Record a host-key rejection in the shared slot so the connect wrapper can
/// surface the typed reason after the handshake aborts. Best-effort: a
/// poisoned lock must not panic inside the russh handler.
fn record_host_key_rejection(
    slot: &StdMutex<Option<HostKeyRejection>>,
    rejection: HostKeyRejection,
) {
    if let Ok(mut guard) = slot.lock() {
        *guard = Some(rejection);
    }
}

impl SshClient {
    const EXEC_COMMAND_TIMEOUT: Duration = Duration::from_secs(300);
    const EXEC_CLOSE_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);
    const EXEC_WAIT_TICK: Duration = Duration::from_millis(250);

    pub fn new(output_tx: mpsc::Sender<Vec<u8>>) -> Self {
        Self {
            shell_reader: Arc::new(ShellReader::default()),
            remote_forwards: Default::default(),
            session: Arc::new(Mutex::new(None)),
            channel: Arc::new(Mutex::new(None)),
            output_tx,
            shell_channel_id: Arc::new(Mutex::new(None)),
            server_key: Arc::new(Mutex::new(None)),
            host_key_check: None,
            host_key_rejection: Arc::new(StdMutex::new(None)),
        }
    }

    pub(crate) fn remote_forward_registry(
        &self,
    ) -> crate::tunnel::remote_forward::RemoteForwardRegistry {
        self.remote_forwards.clone()
    }

    /// Attach the host-key verification policy (which store to consult and
    /// under which host:port identity). Must be called before connecting;
    /// without it the handshake fails closed.
    pub fn set_host_key_check(&mut self, check: HostKeyCheck) {
        self.host_key_check = Some(check);
    }

    /// Take the host-key rejection recorded by the most recent connect
    /// attempt, if the handshake was aborted by the verification policy.
    pub async fn take_host_key_rejection(&self) -> Option<HostKeyRejection> {
        self.host_key_rejection
            .lock()
            .ok()
            .and_then(|mut slot| slot.take())
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

    /// Perform the SSH transport handshake (key exchange + host-key
    /// verification) without authenticating. Returns the established session
    /// handle, or a typed host-key error when the verification policy refused
    /// the server's key.
    ///
    /// Shared by `connect_password`, `connect_key` and `connect_handshake` so
    /// every connection path enforces the exact same TOFU policy.
    async fn establish_connection(
        &self,
        host: &str,
        port: u16,
        tcp_timeout: Duration,
    ) -> Result<client::Handle<ClientHandler>> {
        // Reset per-attempt host-key state so retries start clean.
        if let Ok(mut slot) = self.host_key_rejection.lock() {
            *slot = None;
        }
        self.clear_server_key().await;

        // Configure SSH client with proper timeout and keepalive settings
        // Without these, connections may be dropped during the handshake phase
        let config = Arc::new(client::Config {
            inactivity_timeout: Some(Duration::from_secs(60)),
            keepalive_interval: Some(Duration::from_secs(10)),
            keepalive_max: 5,
            ..Default::default()
        });
        debug!("[SSH] Config: inactivity_timeout=60s, keepalive_interval=10s, keepalive_max=5");

        let handler = ClientHandler {
            remote_forwards: self.remote_forwards.clone(),
            server_key: self.server_key.clone(),
            host_key_check: self.host_key_check.clone(),
            host_key_rejection: self.host_key_rejection.clone(),
        };

        info!(
            "[SSH] Attempting TCP connection to {}:{} (timeout: {}s)...",
            host,
            port,
            tcp_timeout.as_secs()
        );
        match tokio::time::timeout(tcp_timeout, client::connect(config, (host, port), handler))
            .await
        {
            Err(_) => Err(anyhow!(
                "TCP connection to {}:{} timed out after {}s (check network/Tailscale/VPN status)",
                host,
                port,
                tcp_timeout.as_secs()
            )),
            Ok(Err(err)) => {
                // The handshake failed: if the host-key policy aborted it,
                // surface the structured HOST_KEY_* error instead of the
                // generic russh failure. A jump bridge's TCP endpoint is not
                // the identity whose host key the user needs to approve.
                if let Some(rejection) = self.host_key_rejection() {
                    return Err(self.host_key_error(&rejection, host, port));
                }
                Err(err).with_context(|| format!("Failed to connect to {}:{}", host, port))
            }
            Ok(Ok(session)) => {
                // Belt and braces: never hand out a session whose key was
                // rejected (or unverified), even if russh returned Ok.
                if let Some(rejection) = self.host_key_rejection() {
                    return Err(self.host_key_error(&rejection, host, port));
                }
                Ok(session)
            }
        }
    }

    fn host_key_error(&self, rejection: &HostKeyRejection, host: &str, port: u16) -> anyhow::Error {
        let (host, port) = self
            .host_key_check
            .as_ref()
            .map(|check| (check.host(), check.port()))
            .unwrap_or((host, port));
        anyhow!(rejection.error_message(host, port))
    }

    /// Peek (without consuming) the recorded host-key rejection.
    fn host_key_rejection(&self) -> Option<HostKeyRejection> {
        self.host_key_rejection
            .lock()
            .ok()
            .and_then(|slot| slot.clone())
    }

    /// Perform a handshake-only connection (key exchange + host-key check,
    /// NO authentication) to learn whether the server's host key is trusted.
    /// Used by the TOFU probe before any credentials are sent. The connection
    /// is closed immediately afterwards.
    pub async fn connect_handshake(&mut self, host: &str, port: u16) -> Result<ServerKeyInfo> {
        let session = self
            .establish_connection(host, port, PROBE_CONNECT_TIMEOUT)
            .await?;

        let key_info = self.get_server_key().await.ok_or_else(|| {
            anyhow!(
                "Host key was not captured during the handshake with {}:{}",
                host,
                port
            )
        })?;

        // Probe complete: close the connection without authenticating.
        let _ = session
            .disconnect(Disconnect::ByApplication, "", "en")
            .await;

        Ok(key_info)
    }

    /// Get a clone of the channel Arc for sharing with other tasks
    pub fn channel_handle(&self) -> Arc<Mutex<Option<ChannelWriteHalf<client::Msg>>>> {
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

        // Perform the SSH transport handshake (host-key verification happens
        // here, before any authentication material is sent).
        let mut session = self
            .establish_connection(host, port, CONNECT_TIMEOUT)
            .await?;

        info!("[SSH] TCP connection established, starting password authentication...");
        let authenticated = tokio::time::timeout(Duration::from_secs(60), async {
            if session.authenticate_password(username, password).await?.success() {
                return Ok::<bool, anyhow::Error>(true);
            }
            // PAM servers commonly expose a password via keyboard-interactive.
            // Only answer a single hidden password prompt; never submit a saved
            // password as an OTP, an echoed response, or a password-change answer.
            use client::KeyboardInteractiveAuthResponse as Response;
            let mut response = session.authenticate_keyboard_interactive_start(username, None).await?;
            let mut sent_password = false;
            for _ in 0..4 {
                response = match response {
                    Response::Success => return Ok(true),
                    Response::Failure { .. } => return Ok(false),
                    Response::InfoRequest { prompts, .. } if prompts.is_empty() => {
                        session.authenticate_keyboard_interactive_respond(Vec::new()).await?
                    }
                    Response::InfoRequest { prompts, .. } if !sent_password
                        && prompts.len() == 1 && !prompts[0].echo
                        && prompts[0].prompt.to_ascii_lowercase().contains("password") => {
                        sent_password = true;
                        session.authenticate_keyboard_interactive_respond(vec![password.to_string()]).await?
                    }
                    Response::InfoRequest { .. } => {
                        return Err(anyhow!("Server requires interactive authentication not supported by saved-password login"));
                    }
                };
            }
            Err(anyhow!("Too many keyboard-interactive authentication rounds"))
        }).await.context("SSH authentication timed out after 60s")??;

        if !authenticated {
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

        // Perform the SSH transport handshake (host-key verification happens
        // here, before any authentication material is sent).
        let mut session = self
            .establish_connection(host, port, CONNECT_TIMEOUT)
            .await?;

        info!("[SSH] TCP connection established, parsing private key...");
        // Normalize empty passphrases from any caller (saved credentials, IPC,
        // MCP) to None so unencrypted keys decode correctly.
        let passphrase = passphrase.filter(|pass| !pass.is_empty());
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
        let hash = session
            .best_supported_rsa_hash()
            .await?
            .flatten()
            .or(Some(HashAlg::Sha512));
        let auth_result = tokio::time::timeout(
            Duration::from_secs(60),
            session.authenticate_publickey(
                username,
                PrivateKeyWithHashAlg::new(Arc::new(key_pair), hash),
            ),
        )
        .await
        .context("SSH key authentication timed out after 60s")?
        .with_context(|| format!("Public key authentication failed for user '{}'", username))?;

        if !auth_result.success() {
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
        self.shell_reader.abort();
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
                let _ = channel.close().await;
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
        session_guard
            .as_ref()
            .is_some_and(|session| !session.is_closed())
    }

    pub async fn is_shell_open(&self) -> bool {
        self.shell_channel_id.lock().await.is_some() && self.is_connected().await
    }

    async fn session_channel(&self) -> Result<Channel<client::Msg>> {
        tokio::time::timeout(Duration::from_secs(15), async {
            let guard = self.session.lock().await;
            guard
                .as_ref()
                .ok_or_else(|| anyhow!("SSH client not connected"))?
                .channel_open_session()
                .await
                .map_err(anyhow::Error::from)
        })
        .await
        .context("SSH channel open timed out")?
    }

    async fn channel_accepted(channel: &mut Channel<client::Msg>, operation: &str) -> Result<()> {
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                match channel.wait().await {
                    Some(ChannelMsg::Success) => return Ok(()),
                    Some(ChannelMsg::Failure | ChannelMsg::Close) | None => {
                        return Err(anyhow!("Server rejected {operation}"))
                    }
                    // Window adjustments may arrive before the request reply.
                    Some(_) => (),
                }
            }
        })
        .await
        .with_context(|| format!("Server did not acknowledge {operation}"))?
    }

    /// Split the PTY into a writer and a continuously consumed message stream.
    pub async fn open_shell(&mut self, pty_config: Option<PtyConfig>) -> Result<()> {
        anyhow::ensure!(self.channel.lock().await.is_none(), "Shell already opened");
        let mut channel = self
            .session_channel()
            .await
            .context("Failed to open shell channel")?;
        let pty = pty_config.unwrap_or_default();
        let setup = async {
            channel
                .request_pty(
                    true,
                    &pty.term,
                    pty.cols,
                    pty.rows,
                    pty.pix_width,
                    pty.pix_height,
                    &[],
                )
                .await?;
            Self::channel_accepted(&mut channel, "PTY request").await?;
            channel.request_shell(true).await?;
            Self::channel_accepted(&mut channel, "shell request").await
        }
        .await;
        if let Err(error) = setup {
            let _ = channel.close().await;
            return Err(error);
        }
        let channel_id = channel.id();
        *self.shell_channel_id.lock().await = Some(channel_id);
        let (mut reader, writer) = channel.split();
        *self.channel.lock().await = Some(writer);
        let output = self.output_tx.clone();
        let identity = self.shell_channel_id.clone();
        let channel_state = self.channel.clone();
        let task = tokio::spawn(async move {
            while let Some(message) = reader.wait().await {
                match message {
                    ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => {
                        if output.send(data.to_vec()).await.is_err() {
                            break;
                        }
                    }
                    ChannelMsg::Close => break,
                    _ => (),
                }
            }
            let mut current = identity.lock().await;
            if *current == Some(channel_id) {
                *channel_state.lock().await = None;
                *current = None;
            }
        });
        *self
            .shell_reader
            .0
            .lock()
            .map_err(|_| anyhow!("Shell reader state unavailable"))? = Some(task);
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

        let mut channel = self
            .session_channel()
            .await
            .context("Failed to open SFTP channel")?;

        channel
            .request_subsystem(true, "sftp")
            .await
            .with_context(|| "Failed to request SFTP subsystem")?;

        info!("[SSH] SFTP subsystem requested, initializing session...");

        Self::channel_accepted(&mut channel, "SFTP subsystem").await?;
        let sftp = tokio::time::timeout(
            Duration::from_secs(15),
            SftpSession::new(channel.into_stream()),
        )
        .await
        .context("SFTP initialization timed out")?
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
        let result = self.exec_command_result(command, stdin_data).await?;
        // Some appliances omit exit-status entirely. Preserve their output,
        // but never hide an explicitly reported failure from legacy callers.
        if result.exit_code > 0 {
            let detail: String = result.output.chars().take(4096).collect();
            return Err(anyhow!(
                "Remote command exited with status {}: {}",
                result.exit_code,
                detail
            ));
        }
        Ok(result.output)
    }

    pub async fn exec_command_result(
        &self,
        command: &str,
        stdin_data: Option<&str>,
    ) -> Result<CommandResult> {
        self.exec_command_result_with_limits(
            command,
            stdin_data,
            Self::EXEC_COMMAND_TIMEOUT,
            8 * 1024 * 1024,
        )
        .await
    }

    pub(crate) async fn exec_command_result_with_limits(
        &self,
        command: &str,
        stdin_data: Option<&str>,
        timeout: Duration,
        max_output: usize,
    ) -> Result<CommandResult> {
        // Commands and their output can contain credentials; only log sizes.
        debug!(
            "[SSH] Executing command via exec channel ({} bytes)",
            command.len()
        );

        // Open the channel while holding the russh handle lock, then release it.
        // The returned Channel owns its own sender/receiver, so command execution
        // must not block unrelated SFTP/tunnel/session operations from opening
        // their own channels.
        let mut channel = self
            .session_channel()
            .await
            .context("Failed to open exec channel")?;

        // Execute the command (not a shell, just exec)
        if let Err(error) = channel.exec(true, command).await {
            let _ = channel.close().await;
            return Err(error).context("Failed to send exec request");
        }

        // This is a non-interactive exec channel. Even without supplied input,
        // signal EOF so commands reading stdin do not wait until the timeout.
        if let Some(data) = stdin_data.filter(|data| !data.is_empty()) {
            let payload = format!("{data}\n");
            if let Err(error) = channel.data(payload.as_bytes()).await {
                let _ = channel.close().await;
                return Err(error).context("Failed to write command stdin");
            }
        }
        if let Err(error) = channel.eof().await {
            let _ = channel.close().await;
            return Err(error).context("Failed to finish command stdin");
        }

        // Collect output
        let mut output = Vec::new();
        let start = tokio::time::Instant::now();
        let mut exit_status = None;
        let mut remote_closed = false;
        let mut timed_out = false;
        let mut last_message_at = start;

        loop {
            let elapsed = start.elapsed();
            if elapsed >= timeout {
                timed_out = true;
                warn!("[SSH] Command execution timed out after {:?}", timeout);
                break;
            }

            let remaining = timeout.saturating_sub(elapsed);
            let wait_for = remaining.min(Self::EXEC_WAIT_TICK);

            match tokio::time::timeout(wait_for, channel.wait()).await {
                Ok(Some(msg)) => {
                    last_message_at = tokio::time::Instant::now();
                    match msg {
                        ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => {
                            // Protect the app and IPC peers from unbounded output.
                            if output.len().saturating_add(data.len()) > max_output {
                                let _ = channel.close().await;
                                return Err(anyhow!(
                                    "Command output exceeded the {} MiB limit",
                                    max_output / (1024 * 1024)
                                ));
                            }
                            output.extend_from_slice(&data);
                        }
                        ChannelMsg::Failure => {
                            let _ = channel.close().await;
                            return Err(anyhow!("Server rejected the exec request"));
                        }
                        ChannelMsg::Eof => {
                            debug!("[SSH] Received EOF from exec channel");
                            // EOF only closes stdout/stderr, not the process.
                            // Await its exit status or close, bounded by timeout.
                        }
                        ChannelMsg::ExitStatus {
                            exit_status: status,
                        } => {
                            debug!("[SSH] Command exit status: {}", status);
                            // Continue to collect any remaining output and wait for Close.
                            exit_status = Some(status);
                        }
                        ChannelMsg::ExitSignal { .. } => {
                            exit_status = Some(255);
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
                    if exit_status.is_some()
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
            return Err(anyhow!("Command timed out after {}s", timeout.as_secs()));
        }

        let output_str = String::from_utf8_lossy(&output).to_string();
        debug!("[SSH] Command output received ({} bytes)", output_str.len());

        Ok(CommandResult {
            output: output_str,
            exit_code: exit_status
                .and_then(|code| i32::try_from(code).ok())
                .unwrap_or(-1),
        })
    }
}
