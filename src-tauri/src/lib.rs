#[cfg(test)]
mod acl_tests;
pub mod commands;
pub mod install;
pub mod ipc;
pub mod local_shell;
pub mod logging;
pub mod mcp;
pub mod remote_tools;
pub mod session;
pub mod sftp;
pub mod ssh;
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
    clear_fingerprints,
    delete_credential,
    delete_fingerprint,
    delete_fingerprint_by_id,
    delete_group,
    delete_recording,
    delete_server,
    detect_ai_tools,
    get_app_version,
    get_credential,
    // Fingerprint commands
    get_fingerprint,
    get_groups,
    get_recording_content,
    get_server_status,
    get_servers,
    get_session_recording_id,
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

use local_shell::LocalShellManager;
use tauri::{Emitter, Manager};

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
fn get_agent_gateway_status(gateway: tauri::State<'_, AgentGateway>) -> AgentGatewayStatus {
    gateway.status()
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

    let gateway_database = database.clone();
    let gateway_session_manager = session_manager.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .setup(move |app| {
            let app_handle = app.handle().clone();
            let activity_emitter = Arc::new(move |event| {
                let _ = app_handle.emit("agent-gateway-activity", event);
            });
            let gateway = AgentGateway::start(
                gateway_database.clone(),
                gateway_session_manager.clone(),
                activity_emitter,
            )?;
            app.manage(gateway);
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
            open_external_url,
            get_app_version,
            get_agent_gateway_status,
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
            pick_directory_for_upload,
            pick_download_directory,
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
