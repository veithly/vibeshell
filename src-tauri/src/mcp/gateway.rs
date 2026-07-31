use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use directories::ProjectDirs;
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use crate::session::SessionManager;
use crate::storage::Database;

use super::approval::AgentApprovalManager;
use super::guard::SharedAgentInputTracker;
use super::server::{AgentActivityEvent, McpServer, TerminalInputEvent, MCP_PROTOCOL_VERSION};

pub const GATEWAY_SCHEMA_VERSION: u32 = 1;
pub const GATEWAY_MANIFEST_FILE: &str = "agent-gateway.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GatewayStatus {
    Running,
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayManifest {
    pub schema_version: u32,
    pub gateway_version: String,
    pub mcp_protocol_version: String,
    pub app_version: String,
    pub status: GatewayStatus,
    pub pid: Option<u32>,
    pub endpoint: Option<String>,
    pub token: Option<String>,
    pub started_at: Option<i64>,
    pub platform: String,
    pub launch_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentGatewayStatus {
    pub running: bool,
    pub endpoint: Option<String>,
    pub manifest_path: String,
    pub pid: Option<u32>,
    pub protocol_version: String,
}

#[derive(Debug, Clone)]
struct LiveGatewayInfo {
    endpoint: String,
    pid: u32,
}

pub struct AgentGateway {
    manifest_path: PathBuf,
    live: Arc<Mutex<Option<LiveGatewayInfo>>>,
    shutdown_tx: Mutex<Option<oneshot::Sender<()>>>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl AgentGateway {
    pub fn start(
        database: Arc<Database>,
        session_manager: Arc<SessionManager>,
        activity_emitter: Arc<dyn Fn(AgentActivityEvent) + Send + Sync>,
        terminal_input_emitter: Arc<dyn Fn(TerminalInputEvent) + Send + Sync>,
        approvals: Arc<AgentApprovalManager>,
        agent_input_tracker: Arc<SharedAgentInputTracker>,
    ) -> Result<Self> {
        Self::start_at_path(
            database,
            session_manager,
            activity_emitter,
            terminal_input_emitter,
            approvals,
            agent_input_tracker,
            gateway_manifest_path()?,
        )
    }

    fn start_at_path(
        database: Arc<Database>,
        session_manager: Arc<SessionManager>,
        activity_emitter: Arc<dyn Fn(AgentActivityEvent) + Send + Sync>,
        terminal_input_emitter: Arc<dyn Fn(TerminalInputEvent) + Send + Sync>,
        approvals: Arc<AgentApprovalManager>,
        agent_input_tracker: Arc<SharedAgentInputTracker>,
        manifest_path: PathBuf,
    ) -> Result<Self> {
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let (ready_tx, ready_rx) = mpsc::sync_channel::<Result<LiveGatewayInfo, String>>(1);
        let live = Arc::new(Mutex::new(None));
        let thread_live = live.clone();
        let thread_manifest_path = manifest_path.clone();

        let thread = std::thread::Builder::new()
            .name("vibeshell-agent-gateway".to_string())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        let _ = ready_tx.send(Err(format!(
                            "Failed to create Agent Gateway runtime: {}",
                            error
                        )));
                        return;
                    }
                };

                runtime.block_on(async move {
                    let listener = match tokio::net::TcpListener::bind(("127.0.0.1", 0)).await {
                        Ok(listener) => listener,
                        Err(error) => {
                            let _ = ready_tx
                                .send(Err(format!("Failed to bind Agent Gateway: {}", error)));
                            return;
                        }
                    };
                    let address = match listener.local_addr() {
                        Ok(address) => address,
                        Err(error) => {
                            let _ = ready_tx.send(Err(format!(
                                "Failed to resolve Agent Gateway address: {}",
                                error
                            )));
                            return;
                        }
                    };

                    let endpoint = format!("http://127.0.0.1:{}", address.port());
                    let token = generate_token();
                    let pid = std::process::id();
                    let running_manifest =
                        GatewayManifest::running(endpoint.clone(), token.clone(), pid);

                    if let Err(error) =
                        write_manifest_atomic(&thread_manifest_path, &running_manifest)
                    {
                        let _ = ready_tx.send(Err(format!(
                            "Failed to publish Agent Gateway manifest: {}",
                            error
                        )));
                        return;
                    }

                    let info = LiveGatewayInfo {
                        endpoint: endpoint.clone(),
                        pid,
                    };
                    *thread_live.lock().expect("gateway status lock poisoned") = Some(info.clone());
                    let _ = ready_tx.send(Ok(info));

                    let server = McpServer::new(database, session_manager)
                        .with_activity_emitter(activity_emitter)
                        .with_terminal_input_emitter(terminal_input_emitter)
                        .with_approvals(approvals)
                        .with_agent_input_tracker(agent_input_tracker);
                    let serve_result = axum::serve(listener, server.router(token))
                        .with_graceful_shutdown(async {
                            let _ = shutdown_rx.await;
                        })
                        .await;

                    if let Err(error) = serve_result {
                        log::error!("[Agent Gateway] Server stopped with error: {}", error);
                    }
                    *thread_live.lock().expect("gateway status lock poisoned") = None;
                    if let Err(error) =
                        write_manifest_atomic(&thread_manifest_path, &GatewayManifest::stopped())
                    {
                        log::warn!("[Agent Gateway] Could not mark manifest stopped: {}", error);
                    }
                });
            })
            .context("Failed to spawn Agent Gateway thread")?;

        let info = ready_rx
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| anyhow!("Timed out while starting Agent Gateway"))?
            .map_err(anyhow::Error::msg)?;
        log::info!(
            "[Agent Gateway] Listening on {} (pid {})",
            info.endpoint,
            info.pid
        );

        Ok(Self {
            manifest_path,
            live,
            shutdown_tx: Mutex::new(Some(shutdown_tx)),
            thread: Mutex::new(Some(thread)),
        })
    }

    pub fn status(&self) -> AgentGatewayStatus {
        let live = self.live.lock().expect("gateway status lock poisoned");
        AgentGatewayStatus {
            running: live.is_some(),
            endpoint: live.as_ref().map(|info| info.endpoint.clone()),
            manifest_path: self.manifest_path.to_string_lossy().into_owned(),
            pid: live.as_ref().map(|info| info.pid),
            protocol_version: MCP_PROTOCOL_VERSION.to_string(),
        }
    }

    pub fn shutdown(&self) {
        if let Some(sender) = self
            .shutdown_tx
            .lock()
            .expect("gateway shutdown lock poisoned")
            .take()
        {
            let _ = sender.send(());
        }
    }
}

impl Drop for AgentGateway {
    fn drop(&mut self) {
        self.shutdown();
        if let Some(thread) = self
            .thread
            .lock()
            .expect("gateway thread lock poisoned")
            .take()
        {
            let _ = thread.join();
        }
    }
}

impl GatewayManifest {
    fn running(endpoint: String, token: String, pid: u32) -> Self {
        Self {
            schema_version: GATEWAY_SCHEMA_VERSION,
            gateway_version: env!("CARGO_PKG_VERSION").to_string(),
            mcp_protocol_version: MCP_PROTOCOL_VERSION.to_string(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            status: GatewayStatus::Running,
            pid: Some(pid),
            endpoint: Some(endpoint),
            token: Some(token),
            started_at: Some(chrono::Utc::now().timestamp_millis()),
            platform: std::env::consts::OS.to_string(),
            launch_path: resolve_launch_path().to_string_lossy().into_owned(),
        }
    }

    fn stopped() -> Self {
        Self {
            schema_version: GATEWAY_SCHEMA_VERSION,
            gateway_version: env!("CARGO_PKG_VERSION").to_string(),
            mcp_protocol_version: MCP_PROTOCOL_VERSION.to_string(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            status: GatewayStatus::Stopped,
            pid: None,
            endpoint: None,
            token: None,
            started_at: None,
            platform: std::env::consts::OS.to_string(),
            launch_path: resolve_launch_path().to_string_lossy().into_owned(),
        }
    }
}

pub fn gateway_manifest_path() -> Result<PathBuf> {
    let project_dirs = ProjectDirs::from("com", "vibeshell", "VibeShell")
        .ok_or_else(|| anyhow!("Could not determine VibeShell data directory"))?;
    Ok(project_dirs.data_dir().join(GATEWAY_MANIFEST_FILE))
}

fn resolve_launch_path() -> PathBuf {
    #[cfg(target_os = "linux")]
    if let Some(path) = std::env::var_os("APPIMAGE").filter(|value| !value.is_empty()) {
        return PathBuf::from(path);
    }

    std::env::current_exe().unwrap_or_else(|_| PathBuf::from("vibeshell"))
}

fn generate_token() -> String {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn write_manifest_atomic(path: &Path, manifest: &GatewayManifest) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("Gateway manifest path has no parent"))?;
    fs::create_dir_all(parent)
        .with_context(|| format!("Failed to create Gateway directory {}", parent.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    }

    let temp_path = parent.join(format!(
        ".{}.{}.tmp",
        GATEWAY_MANIFEST_FILE,
        uuid::Uuid::new_v4()
    ));
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    let write_result = (|| -> Result<()> {
        let mut file = options
            .open(&temp_path)
            .with_context(|| format!("Failed to create {}", temp_path.display()))?;
        serde_json::to_writer_pretty(&mut file, manifest)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        replace_manifest(&temp_path, path)
            .with_context(|| format!("Failed to replace Gateway manifest {}", path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    })();

    if write_result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    write_result
}

#[cfg(not(windows))]
fn replace_manifest(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
fn replace_manifest(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source: Vec<u16> = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_tokens_are_random_and_256_bit() {
        let first = generate_token();
        let second = generate_token();
        assert_ne!(first, second);
        assert_eq!(URL_SAFE_NO_PAD.decode(first).unwrap().len(), 32);
    }

    #[test]
    fn manifest_write_is_atomic_and_private() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(GATEWAY_MANIFEST_FILE);
        let manifest = GatewayManifest::stopped();
        write_manifest_atomic(&path, &manifest).unwrap();

        let stored: GatewayManifest = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(stored.schema_version, GATEWAY_SCHEMA_VERSION);
        assert_eq!(stored.status, GatewayStatus::Stopped);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn gateway_lifecycle_publishes_running_then_stopped_manifest() {
        let temp = tempfile::tempdir().unwrap();
        let manifest_path = temp.path().join(GATEWAY_MANIFEST_FILE);
        let database = Arc::new(Database::new_at(temp.path().join("gateway.db")).unwrap());
        let session_manager = Arc::new(SessionManager::new(database.clone()));
        let emitter = Arc::new(|_event: AgentActivityEvent| {});
        let terminal_emitter = Arc::new(|_event: TerminalInputEvent| {});
        let approvals = Arc::new(AgentApprovalManager::new(Arc::new(|_| {})));
        let agent_input_tracker = Arc::new(SharedAgentInputTracker::default());

        let gateway = AgentGateway::start_at_path(
            database,
            session_manager,
            emitter,
            terminal_emitter,
            approvals,
            agent_input_tracker,
            manifest_path.clone(),
        )
        .unwrap();
        let status = gateway.status();
        assert!(status.running);
        assert!(status
            .endpoint
            .as_deref()
            .is_some_and(|endpoint| endpoint.starts_with("http://127.0.0.1:")));

        let running: GatewayManifest =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        assert_eq!(running.status, GatewayStatus::Running);
        assert!(running
            .token
            .as_deref()
            .is_some_and(|token| !token.is_empty()));

        gateway.shutdown();
        drop(gateway);
        let stopped: GatewayManifest =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        assert_eq!(stopped.status, GatewayStatus::Stopped);
        assert!(stopped.endpoint.is_none());
        assert!(stopped.token.is_none());
        assert!(!stopped.launch_path.is_empty());
    }
}
