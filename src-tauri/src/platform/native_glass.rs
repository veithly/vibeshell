//! Use native Liquid Glass on macOS 26+, where the older sidebar vibrancy
//! material has become nearly opaque. Keep the configuration's original
//! NSVisualEffectView as the supported fallback on earlier macOS versions.

use tauri::Manager;
use window_vibrancy::{LiquidGlassOptions, NSGlassEffectViewStyle};

pub(crate) fn configure_native_glass(app: &tauri::App) {
    let Some(window) = app.get_webview_window("main") else {
        log::warn!("[Glass] Main window not available for native material setup");
        return;
    };
    configure_native_glass_window(&window);
}

/// Called on the main thread for both the primary and detached workspaces.
pub(crate) fn configure_native_glass_window(window: &tauri::WebviewWindow) {
    match window_vibrancy::apply_liquid_glass(
        window,
        LiquidGlassOptions::new(NSGlassEffectViewStyle::Regular)
            .radius(14.0)
            .opaque(false),
    ) {
        Ok(()) => {
            // Tauri's set_effects(None) currently only clears effects on
            // Windows: on macOS it returns Ok without removing anything.
            // Both window-vibrancy versions use the same tagged legacy view.
            // Clear it explicitly *after* adding Liquid Glass. Otherwise
            // the stacked sidebar view renders as an opaque surface.
            match window_vibrancy::clear_vibrancy(window) {
                Ok(true) => {
                    log::info!("[Glass] Native Liquid Glass enabled; legacy sidebar removed")
                }
                Ok(false) => log::warn!("[Glass] Liquid Glass enabled; no legacy sidebar found"),
                Err(error) => {
                    log::warn!("[Glass] Could not clear the legacy sidebar effect: {error}")
                }
            }
        }
        Err(window_vibrancy::Error::UnsupportedPlatformVersion(_)) => {
            log::info!("[Glass] Using the legacy native sidebar material on older macOS");
        }
        Err(error) => {
            // A graphics initialization issue must not prevent terminal or
            // SSH access. The original sidebar effect remains as a fallback.
            log::warn!("[Glass] Liquid Glass initialization failed: {error}");
        }
    }
}
