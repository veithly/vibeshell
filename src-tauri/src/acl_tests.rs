use tauri::ipc::Origin;

#[test]
fn event_plugin_listen_is_allowed_for_main_window_and_webview() {
    let mut context: tauri::Context<tauri::Wry> = tauri::generate_context!();
    let authority = context.runtime_authority_mut();

    for command in [
        "plugin:event|listen",
        "plugin:event|unlisten",
        "plugin:event|emit",
    ] {
        assert!(
            authority
                .resolve_access(command, "main", "main", &Origin::Local)
                .is_some(),
            "{command} should be allowed for main window/webview with local origin"
        );
    }
}
