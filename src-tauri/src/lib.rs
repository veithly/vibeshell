#[cfg(test)]
mod acl_tests;
pub mod commands;
pub mod install;
pub mod ipc;
pub mod local_shell;
pub mod logging;
pub mod mcp;
pub mod session;
pub mod sftp;
pub mod ssh;
pub mod storage;
pub mod tunnel;

use std::io::Write;
use std::sync::Arc;

pub use ipc::{IpcServer, IpcServerRunError};
pub use mcp::McpServer;
pub use session::SessionManager;
pub use sftp::SftpClient;
pub use storage::Database;

use commands::{
    add_group,
    add_server,
    add_vshell_to_path,
    clear_fingerprints,
    delete_credential,
    delete_fingerprint,
    delete_fingerprint_by_id,
    delete_group,
    delete_recording,
    delete_server,
    detect_ai_tools,
    get_credential,
    // Fingerprint commands
    get_fingerprint,
    get_groups,
    get_recording_content,
    get_server_status,
    get_servers,
    get_session_recording_id,
    get_vshell_path,
    install_to_tool,
    is_session_recording,
    list_fingerprints,
    list_recordings,
    local_shell_attach,
    local_shell_create,
    local_shell_detach,
    local_shell_get_default,
    local_shell_kill,
    local_shell_kill_all,
    local_shell_list_sessions,
    // Local shell commands
    local_shell_list_shells,
    local_shell_resize,
    local_shell_send_bytes,
    local_shell_send_input,
    pick_download_directory,
    pick_file_for_upload,
    // Dialog commands
    pick_ssh_key_file,
    read_ssh_key_file,
    save_credential,
    save_fingerprint,
    session_attach,
    session_connect,
    session_create,
    session_detach,
    session_kill,
    session_kill_all,
    session_list,
    session_resize,
    session_send_bytes,
    session_send_input,
    sftp_compress,
    sftp_delete,
    sftp_download_file,
    sftp_extract,
    // SFTP commands
    sftp_init,
    sftp_list_dir,
    sftp_mkdir,
    sftp_pwd,
    sftp_read_file,
    sftp_rename,
    sftp_stat,
    sftp_upload_file,
    sftp_write_file,
    snippet_add,
    snippet_delete,
    // Snippet commands
    snippet_list,
    snippet_search,
    snippet_update,
    // Logging/recording commands
    start_recording,
    stop_recording,
    touch_fingerprint,
    tunnel_config_add,
    tunnel_config_delete,
    // Tunnel commands
    tunnel_config_list,
    tunnel_config_update,
    tunnel_list_active,
    tunnel_start,
    tunnel_stop,
    tunnel_stop_all_for_session,
    uninstall_from_tool,
    update_server,
    verify_fingerprint,
    FingerprintState,
    SessionAccessMode,
    SessionAccessState,
    SftpState,
};

use local_shell::LocalShellManager;

/// Returns the current version of VibeShell
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Greet command - example Tauri command
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! Welcome to VibeShell.", name)
}

/// Open developer tools (F12)
#[tauri::command]
fn open_devtools(webview: tauri::WebviewWindow) {
    webview.open_devtools();
}

fn flush_logs_before_exit() {
    log::logger().flush();
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
}

fn should_exit_on_ipc_error(error: &IpcServerRunError) -> bool {
    matches!(
        error,
        IpcServerRunError::ListenerSetup(_) | IpcServerRunError::ListenerBind(_)
    )
}

fn handle_ipc_server_error(
    error: &IpcServerRunError,
    access_state: &SessionAccessState,
) -> Option<i32> {
    if matches!(
        crate::ipc::IpcClient::endpoint_status(),
        crate::ipc::IpcEndpointStatus::Reachable
    ) {
        log::warn!(
            "[IPC] Local server unavailable, switching GUI to remote session mode via {}: {}",
            IpcServer::socket_name_display(),
            error
        );
        access_state.set_mode(SessionAccessMode::Remote);
        return None;
    }

    if should_exit_on_ipc_error(error) {
        log::error!(
            "[IPC] Server startup failed; shutting down GUI to avoid split-brain: {}",
            error
        );
        eprintln!(
            "[IPC] VibeShell failed to start IPC endpoint ({}). The app will exit to prevent isolated session state.",
            error
        );
        flush_logs_before_exit();
        Some(1)
    } else {
        log::error!(
            "[IPC] Server runtime error on {}. Keeping GUI alive: {}",
            IpcServer::socket_name_display(),
            error
        );
        None
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize logging with RUST_LOG environment variable support
    // Set RUST_LOG=debug or RUST_LOG=vibeshell=debug for detailed logs
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("[VibeShell] Starting application v{}", version());

    let database = Arc::new(Database::new().expect("Failed to initialize database"));
    let session_manager = Arc::new(SessionManager::new(database.clone()));
    let sftp_state = Arc::new(SftpState::new());
    let fingerprint_state =
        FingerprintState::new().expect("Failed to initialize fingerprint store");
    let local_shell_manager = Arc::new(LocalShellManager::new());
    let tunnel_manager = Arc::new(tunnel::TunnelManager::new());
    let session_logger = Arc::new(logging::SessionLogger::new(database.clone()));
    let session_access_state = Arc::new(SessionAccessState::new(SessionAccessMode::Local));

    // Start IPC server in a background thread for CLI communication.
    // Single-master policy: if IPC listener cannot be established, terminate
    // this GUI process instead of continuing as an isolated "split-brain" instance.
    if matches!(
        crate::ipc::IpcClient::endpoint_status(),
        crate::ipc::IpcEndpointStatus::Reachable
    ) {
        log::info!(
            "[IPC] Remote master detected on {}; GUI will proxy shared sessions instead of binding locally.",
            IpcServer::socket_name_display()
        );
        session_access_state.set_mode(SessionAccessMode::Remote);
    } else {
        let db = database.clone();
        let sm = session_manager.clone();
        let access = session_access_state.clone();
        std::thread::spawn(move || {
            let ipc_server = IpcServer::new(db, sm);
            if let Err(e) = ipc_server.run() {
                if let Some(code) = handle_ipc_server_error(&e, &access) {
                    std::process::exit(code);
                }
            }
        });
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|_app| {
            // Ensure vshell CLI is accessible in PATH on all platforms.
            // On macOS/Linux: create symlink in /usr/local/bin/ if not already there.
            // On Windows: add vshell directory to user PATH if not already present.
            //   (NSIS installer also does this via nsis-hooks.nsh, but MSI installs
            //   and dev builds need this runtime fallback.)
            #[cfg(not(windows))]
            {
                let vshell_path = install::resolve_vshell_binary();
                let vshell = std::path::Path::new(&vshell_path);
                let link = std::path::Path::new("/usr/local/bin/vshell");
                if vshell.exists() && !link.exists() {
                    if let Err(e) = std::os::unix::fs::symlink(vshell, link) {
                        log::warn!(
                            "[Setup] Could not create /usr/local/bin/vshell symlink: {}",
                            e
                        );
                    } else {
                        log::info!(
                            "[Setup] Created symlink /usr/local/bin/vshell -> {}",
                            vshell_path
                        );
                    }
                }
            }
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                const CREATE_NO_WINDOW: u32 = 0x08000000;

                let vshell_path = install::resolve_vshell_binary();
                let vshell = std::path::Path::new(&vshell_path);
                if vshell.exists() {
                    if let Some(vshell_dir) = vshell.parent() {
                        let dir_str = vshell_dir.to_string_lossy();
                        let already_in_path = std::process::Command::new("powershell")
                            .args([
                                "-NoProfile",
                                "-Command",
                                "[Environment]::GetEnvironmentVariable('PATH', 'User')",
                            ])
                            .creation_flags(CREATE_NO_WINDOW)
                            .output()
                            .ok()
                            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                            .unwrap_or_default()
                            .split(';')
                            .any(|entry| entry.eq_ignore_ascii_case(&dir_str));

                        if !already_in_path {
                            let add_cmd = format!(
                                "$cur = [Environment]::GetEnvironmentVariable('PATH','User'); \
                                 if ($cur) {{ [Environment]::SetEnvironmentVariable('PATH', \"$cur;{}\", 'User') }} \
                                 else {{ [Environment]::SetEnvironmentVariable('PATH', '{}', 'User') }}",
                                dir_str.replace('\'', "''"),
                                dir_str.replace('\'', "''"),
                            );
                            match std::process::Command::new("powershell")
                                .args(["-NoProfile", "-Command", &add_cmd])
                                .creation_flags(CREATE_NO_WINDOW)
                                .status()
                            {
                                Ok(status) if status.success() => {
                                    log::info!(
                                        "[Setup] Added '{}' to user PATH",
                                        dir_str
                                    );
                                }
                                Ok(status) => {
                                    log::warn!(
                                        "[Setup] PowerShell exited with {} while adding vshell to PATH",
                                        status
                                    );
                                }
                                Err(e) => {
                                    log::warn!(
                                        "[Setup] Could not add vshell to PATH: {}",
                                        e
                                    );
                                }
                            }
                        }
                    }
                }
            }
            Ok(())
        })
        .manage(database)
        .manage(session_manager)
        .manage(sftp_state)
        .manage(fingerprint_state)
        .manage(local_shell_manager)
        .manage(tunnel_manager)
        .manage(session_logger)
        .manage(session_access_state)
        .invoke_handler(tauri::generate_handler![
            greet,
            open_devtools,
            // Session commands
            session_list,
            session_create,
            session_connect,
            session_kill,
            session_kill_all,
            session_send_input,
            session_send_bytes,
            session_resize,
            session_attach,
            session_detach,
            get_server_status,
            // AI tool installation commands
            detect_ai_tools,
            install_to_tool,
            uninstall_from_tool,
            get_vshell_path,
            add_vshell_to_path,
            // Server management commands
            get_servers,
            add_server,
            update_server,
            delete_server,
            get_groups,
            add_group,
            delete_group,
            // Credential commands
            save_credential,
            get_credential,
            delete_credential,
            // Dialog commands
            pick_ssh_key_file,
            pick_file_for_upload,
            pick_download_directory,
            read_ssh_key_file,
            // SFTP commands
            sftp_init,
            sftp_list_dir,
            sftp_download_file,
            sftp_upload_file,
            sftp_mkdir,
            sftp_delete,
            sftp_rename,
            sftp_pwd,
            sftp_stat,
            sftp_read_file,
            sftp_write_file,
            sftp_compress,
            sftp_extract,
            // Fingerprint commands
            get_fingerprint,
            save_fingerprint,
            delete_fingerprint,
            delete_fingerprint_by_id,
            list_fingerprints,
            verify_fingerprint,
            touch_fingerprint,
            clear_fingerprints,
            // Local shell commands
            local_shell_list_shells,
            local_shell_get_default,
            local_shell_list_sessions,
            local_shell_create,
            local_shell_send_input,
            local_shell_send_bytes,
            local_shell_resize,
            local_shell_attach,
            local_shell_detach,
            local_shell_kill,
            local_shell_kill_all,
            // Snippet commands
            snippet_list,
            snippet_add,
            snippet_update,
            snippet_delete,
            snippet_search,
            // Tunnel commands
            tunnel_config_list,
            tunnel_config_add,
            tunnel_config_update,
            tunnel_config_delete,
            tunnel_start,
            tunnel_stop,
            tunnel_list_active,
            tunnel_stop_all_for_session,
            // Recording/logging commands
            start_recording,
            stop_recording,
            list_recordings,
            is_session_recording,
            get_session_recording_id,
            delete_recording,
            get_recording_content,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct EndpointEnvGuard {
        previous: Option<String>,
    }

    impl EndpointEnvGuard {
        const SOCKET_NAME_ENV: &'static str = "VIBESHELL_IPC_NAME";

        fn unique() -> Self {
            let previous = std::env::var(Self::SOCKET_NAME_ENV).ok();
            let unique = format!(
                "vibeshell-lib-test-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("clock should be after unix epoch")
                    .as_nanos()
            );
            std::env::set_var(Self::SOCKET_NAME_ENV, unique);
            Self { previous }
        }
    }

    impl Drop for EndpointEnvGuard {
        fn drop(&mut self) {
            match self.previous.take() {
                Some(value) => std::env::set_var(Self::SOCKET_NAME_ENV, value),
                None => std::env::remove_var(Self::SOCKET_NAME_ENV),
            }
        }
    }

    #[test]
    fn handle_ipc_server_error_returns_exit_code_for_listener_failure() {
        let _guard = EndpointEnvGuard::unique();
        let state = SessionAccessState::new(SessionAccessMode::Local);
        let code = handle_ipc_server_error(
            &IpcServerRunError::ListenerBind(std::io::Error::new(
                std::io::ErrorKind::AddrInUse,
                "bind failed",
            )),
            &state,
        );
        assert_eq!(code, Some(1));
    }

    #[test]
    fn handle_ipc_server_error_keeps_gui_alive_for_runtime_error() {
        let state = SessionAccessState::new(SessionAccessMode::Local);
        let code = handle_ipc_server_error(
            &IpcServerRunError::Runtime(anyhow::anyhow!("accept loop interrupted")),
            &state,
        );
        assert_eq!(code, None);
    }

    #[test]
    fn should_exit_on_ipc_error_only_for_startup_failures() {
        assert!(should_exit_on_ipc_error(&IpcServerRunError::ListenerSetup(
            anyhow::anyhow!("name failure")
        )));
        assert!(should_exit_on_ipc_error(&IpcServerRunError::ListenerBind(
            std::io::Error::new(std::io::ErrorKind::AddrInUse, "in use"),
        )));
        assert!(!should_exit_on_ipc_error(&IpcServerRunError::Runtime(
            anyhow::anyhow!("runtime")
        )));
    }
}
