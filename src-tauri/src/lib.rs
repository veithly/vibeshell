#[cfg(test)]
mod acl_tests;
pub mod cloud_sync;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub mod coding_agent;
pub mod commands;
pub mod install;
pub mod ipc;
pub mod local_shell;
pub mod logging;
pub mod mcp;
pub mod platform;
pub mod plugins;
pub mod remote_tools;
pub mod replay;
pub mod session;
pub mod sftp;
pub mod ssh;
pub mod ssh_import;
pub mod storage;
pub mod tunnel;

use std::sync::Arc;

pub use ipc::{IpcServer, IpcServerRunError};
pub use mcp::{AgentGateway, AgentGatewayStatus, McpServer};
pub use session::SessionManager;
pub use sftp::SftpClient;
pub use storage::Database;

use commands::{
    add_group,
    add_server,
    // Agent command guard
    cancel_agent_auto_approve,
    clear_fingerprints,
    cloud_sync_create_vault,
    cloud_sync_export_file,
    cloud_sync_import_file,
    cloud_sync_join_vault,
    cloud_sync_lock,
    cloud_sync_now,
    cloud_sync_status,
    coding_agent_launch,
    coding_agent_list,
    coding_agent_workspace_diff,
    coding_agent_workspace_status,
    delete_credential,
    delete_fingerprint,
    delete_fingerprint_by_id,
    delete_group,
    delete_recording,
    delete_server,
    detect_ai_tools,
    detect_ssh_import_sources,
    get_agent_guard_config,
    get_agent_guard_status,
    get_app_version,
    get_credential,
    // Fingerprint commands
    get_fingerprint,
    get_groups,
    get_recording_content,
    get_runtime_capabilities,
    get_server_status,
    get_servers,
    get_session_recording_id,
    history_clear,
    history_delete,
    history_list,
    history_record,
    history_set_favorite,
    import_ssh_profiles,
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
    open_external_url,
    pick_directory_for_upload,
    pick_download_directory,
    pick_file_for_upload,
    pick_files_for_upload,
    // Dialog commands
    pick_ssh_key_file,
    pick_workspace_directory,
    plugin_execute,
    plugin_import,
    plugin_install,
    plugin_list,
    plugin_set_enabled,
    plugin_uninstall,
    plugin_update_settings,
    preview_ssh_import,
    read_ssh_key_file,
    resolve_agent_approval,
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
    set_agent_guard_config,
    sftp_compress,
    sftp_delete,
    sftp_download_file,
    sftp_extract,
    sftp_get_upload_ignore_config,
    // SFTP commands
    sftp_init,
    sftp_list_dir,
    sftp_mkdir,
    sftp_pwd,
    sftp_read_file,
    sftp_rename,
    sftp_save_upload_ignore_config,
    sftp_stat,
    sftp_upload_directory,
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

use cloud_sync::CloudSyncManager;
use local_shell::LocalShellManager;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use tauri::Emitter;
use tauri::Manager;

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

#[tauri::command]
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn get_agent_gateway_status(gateway: tauri::State<'_, AgentGateway>) -> AgentGatewayStatus {
    gateway.status()
}

#[tauri::command]
#[cfg(any(target_os = "android", target_os = "ios"))]
fn get_agent_gateway_status() -> AgentGatewayStatus {
    AgentGatewayStatus {
        running: false,
        endpoint: None,
        manifest_path: String::new(),
        pid: None,
        protocol_version: String::new(),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize logging with RUST_LOG environment variable support
    // Set RUST_LOG=debug or RUST_LOG=vibeshell=debug for detailed logs
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("[VibeShell] Starting application v{}", version());

    let builder = tauri::Builder::default();

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let builder = builder
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build());

    builder
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_data_dir)?;

            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            platform::copy_legacy_app_data(&app_data_dir)?;

            let database = Arc::new(Database::new_at(platform::database_path(&app_data_dir))?);
            let session_manager = Arc::new(SessionManager::new(database.clone()));
            let sftp_state = Arc::new(SftpState::new());
            let fingerprint_state =
                FingerprintState::new_at(platform::fingerprint_path(&app_data_dir))
                    .map_err(|error| std::io::Error::new(std::io::ErrorKind::Other, error))?;
            let local_shell_manager = Arc::new(LocalShellManager::new());
            let tunnel_manager = Arc::new(tunnel::TunnelManager::new());
            let session_logger = Arc::new(logging::SessionLogger::new(database.clone()));
            let session_access_state = Arc::new(SessionAccessState::new(SessionAccessMode::Local));
            let cloud_sync_manager = Arc::new(CloudSyncManager::new(database.clone())?);
            let agent_input_tracker = Arc::new(mcp::SharedAgentInputTracker::default());

            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            {
                let activity_handle = app.handle().clone();
                let activity_emitter = Arc::new(move |event| {
                    let _ = activity_handle.emit("agent-gateway-activity", event);
                });

                let terminal_handle = app.handle().clone();
                let terminal_input_emitter = Arc::new(move |event| {
                    let _ = terminal_handle.emit("agent-terminal-input", event);
                });

                let persisted_auto_approve_until = database
                    .get_setting(mcp::approval::AUTO_APPROVE_UNTIL_KEY)?
                    .and_then(|value| value.parse::<i64>().ok());
                let approval_handle = app.handle().clone();
                let approval_manager =
                    Arc::new(mcp::AgentApprovalManager::with_auto_approve_until(
                        Arc::new(move |event| match event {
                            mcp::ApprovalEvent::Request(request) => {
                                let _ = approval_handle.emit("agent-approval-request", request);
                            }
                            mcp::ApprovalEvent::Resolved(resolved) => {
                                let _ = approval_handle.emit("agent-approval-resolved", resolved);
                            }
                            mcp::ApprovalEvent::State(state) => {
                                let _ = approval_handle.emit("agent-approval-state", state);
                            }
                        }),
                        persisted_auto_approve_until,
                    ));

                let gateway = AgentGateway::start(
                    database.clone(),
                    session_manager.clone(),
                    activity_emitter,
                    terminal_input_emitter,
                    approval_manager.clone(),
                    agent_input_tracker.clone(),
                )?;
                app.manage(gateway);
                app.manage(approval_manager);
            }

            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            if std::env::var_os("VIBESHELL_SKIP_SKILL_AUTO_INSTALL").is_none() {
                std::thread::spawn(|| {
                    if let Err(error) = crate::install::install_bundled_cli_for_user() {
                        log::warn!("[Install] Could not install native CLI: {}", error);
                    }
                    for result in crate::install::install_to_all() {
                        if !result.success {
                            log::warn!(
                                "[Install] Could not install VibeShell skill for {}: {}",
                                result.tool.name,
                                result.error.unwrap_or_else(|| "unknown error".to_string())
                            );
                        }
                    }
                });
            }

            app.manage(database);
            app.manage(agent_input_tracker);
            app.manage(session_manager);
            app.manage(sftp_state);
            app.manage(fingerprint_state);
            app.manage(local_shell_manager);
            app.manage(tunnel_manager);
            app.manage(session_logger);
            app.manage(session_access_state);
            app.manage(cloud_sync_manager);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            open_devtools,
            open_external_url,
            get_app_version,
            get_runtime_capabilities,
            get_agent_gateway_status,
            // Agent command guard
            cancel_agent_auto_approve,
            get_agent_guard_config,
            get_agent_guard_status,
            resolve_agent_approval,
            set_agent_guard_config,
            // End-to-end encrypted cloud sync
            cloud_sync_create_vault,
            cloud_sync_export_file,
            cloud_sync_import_file,
            cloud_sync_join_vault,
            cloud_sync_lock,
            cloud_sync_status,
            cloud_sync_now,
            // Coding agents launched inside VibeShell PTYs
            coding_agent_list,
            coding_agent_launch,
            coding_agent_workspace_status,
            coding_agent_workspace_diff,
            // Plugin marketplace commands
            plugin_list,
            plugin_install,
            plugin_import,
            plugin_uninstall,
            plugin_set_enabled,
            plugin_update_settings,
            plugin_execute,
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
            // Server management commands
            get_servers,
            add_server,
            update_server,
            delete_server,
            // SSH configuration import commands
            detect_ssh_import_sources,
            preview_ssh_import,
            import_ssh_profiles,
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
            pick_files_for_upload,
            pick_directory_for_upload,
            pick_download_directory,
            pick_workspace_directory,
            read_ssh_key_file,
            // SFTP commands
            sftp_init,
            sftp_list_dir,
            sftp_download_file,
            sftp_upload_file,
            sftp_upload_directory,
            sftp_mkdir,
            sftp_delete,
            sftp_rename,
            sftp_pwd,
            sftp_stat,
            sftp_read_file,
            sftp_write_file,
            sftp_compress,
            sftp_extract,
            sftp_get_upload_ignore_config,
            sftp_save_upload_ignore_config,
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
            // Command history commands
            history_list,
            history_record,
            history_set_favorite,
            history_delete,
            history_clear,
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
