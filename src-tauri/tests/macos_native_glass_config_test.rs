#[cfg(target_os = "macos")]
#[test]
fn macos_window_is_transparent_and_has_a_pre_liquid_glass_fallback() {
    let context: tauri::Context<tauri::Wry> = tauri::generate_context!();
    let config = context.config();
    let main = config
        .app
        .windows
        .iter()
        .find(|window| window.label == "main")
        .expect("primary application window");
    assert!(
        config.app.macos_private_api,
        "a transparent WKWebView needs macOS private API support"
    );
    assert!(
        main.transparent,
        "window must be transparent for behind-window Liquid Glass"
    );
    assert!(
        main.window_effects
            .as_ref()
            .is_some_and(|effect| !effect.effects.is_empty()),
        "legacy native vibrancy must remain as the macOS 11–25 fallback"
    );
}
