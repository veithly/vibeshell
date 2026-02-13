pub mod storage;
pub mod ssh;
pub mod sftp;
pub mod session;
pub mod commands;
pub mod ipc;
pub mod mcp;
pub mod install;
pub mod local_shell;
pub mod tunnel;
pub mod logging;

use std::sync::Arc;

pub use storage::Database;
pub use session::SessionManager;
pub use sftp::SftpClient;
pub use mcp::McpServer;
pub use ipc::IpcServer;

use commands::{
    session_list, session_create, session_connect, session_kill, session_kill_all,
    session_send_input, session_send_bytes, session_resize,
    session_attach, session_detach, get_server_status,
    detect_ai_tools, install_to_tool, uninstall_from_tool,
    get_servers, add_server, update_server, delete_server,
    get_groups, add_group, delete_group,
    save_credential, get_credential, delete_credential,
    // Dialog commands
    pick_ssh_key_file, pick_file_for_upload, pick_download_directory, read_ssh_key_file,
    // SFTP commands
    sftp_init, sftp_list_dir, sftp_download_file, sftp_upload_file,
    sftp_mkdir, sftp_delete, sftp_rename, sftp_pwd, sftp_stat, sftp_read_file,
    sftp_write_file, sftp_compress, sftp_extract,
    SftpState,
    // Fingerprint commands
    get_fingerprint, save_fingerprint, delete_fingerprint, delete_fingerprint_by_id,
    list_fingerprints, verify_fingerprint, touch_fingerprint, clear_fingerprints,
    FingerprintState,
    // Local shell commands
    local_shell_list_shells, local_shell_get_default, local_shell_list_sessions,
    local_shell_create, local_shell_send_input, local_shell_send_bytes,
    local_shell_resize, local_shell_attach, local_shell_detach,
    local_shell_kill, local_shell_kill_all,
    // Snippet commands
    snippet_list, snippet_add, snippet_update, snippet_delete, snippet_search,
    // Tunnel commands
    tunnel_config_list, tunnel_config_add, tunnel_config_update, tunnel_config_delete,
    tunnel_start, tunnel_stop, tunnel_list_active, tunnel_stop_all_for_session,
    // Logging/recording commands
    start_recording, stop_recording, list_recordings, is_session_recording,
    get_session_recording_id, delete_recording, get_recording_content,
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize logging with RUST_LOG environment variable support
    // Set RUST_LOG=debug or RUST_LOG=vibeshell=debug for detailed logs
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info")
    ).init();

    log::info!("[VibeShell] Starting application v{}", version());

    let database = Arc::new(Database::new().expect("Failed to initialize database"));
    let session_manager = Arc::new(SessionManager::new(database.clone()));
    let sftp_state = Arc::new(SftpState::new());
    let fingerprint_state = FingerprintState::new().expect("Failed to initialize fingerprint store");
    let local_shell_manager = Arc::new(LocalShellManager::new());
    let tunnel_manager = Arc::new(tunnel::TunnelManager::new());
    let session_logger = Arc::new(logging::SessionLogger::new(database.clone()));

    // Start IPC server in a background thread for CLI communication
    {
        let db = database.clone();
        let sm = session_manager.clone();
        std::thread::spawn(move || {
            let ipc_server = IpcServer::new(db, sm);
            if let Err(e) = ipc_server.run() {
                log::error!("[IPC] Server error: {}", e);
            }
        });
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(database)
        .manage(session_manager)
        .manage(sftp_state)
        .manage(fingerprint_state)
        .manage(local_shell_manager)
        .manage(tunnel_manager)
        .manage(session_logger)
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
