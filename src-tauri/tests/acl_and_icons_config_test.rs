use std::{
    fs,
    path::{Path, PathBuf},
};

use serde_json::Value;

fn read_json(path: &Path) -> Value {
    let content = fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
    serde_json::from_str(&content)
        .unwrap_or_else(|err| panic!("failed to parse {} as JSON: {err}", path.display()))
}

#[test]
fn capability_allows_event_listener_commands() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let capability_path = manifest_dir.join("capabilities").join("default.json");
    let capability = read_json(&capability_path);

    let permissions = capability["permissions"]
        .as_array()
        .expect("capability permissions must be an array");

    for permission in [
        "core:event:allow-listen",
        "core:event:allow-unlisten",
        "core:event:allow-emit",
    ] {
        assert!(
            permissions
                .iter()
                .any(|entry| entry.as_str() == Some(permission)),
            "missing required permission `{permission}` in {}",
            capability_path.display()
        );
    }
}

#[test]
fn capability_allows_localhost_dev_origins() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let capability_path = manifest_dir.join("capabilities").join("default.json");
    let capability = read_json(&capability_path);

    let urls = capability
        .get("remote")
        .and_then(|remote| remote.get("urls"))
        .and_then(Value::as_array)
        .expect("capability must define remote.urls for dev origins");

    for url in ["http://localhost:*", "http://127.0.0.1:*"] {
        assert!(
            urls.iter().any(|entry| entry.as_str() == Some(url)),
            "missing `{url}` in capability remote URLs"
        );
    }
}

#[test]
fn frontend_uses_vibeshell_favicon() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("src-tauri must live under workspace root")
        .to_path_buf();

    let index_path = workspace_root.join("index.html");
    let index_html = fs::read_to_string(&index_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", index_path.display()));

    assert!(
        index_html.contains("/app-icon.svg"),
        "index.html should reference /app-icon.svg as favicon"
    );
    assert!(
        !index_html.contains("/vite.svg"),
        "index.html should not reference the default Vite favicon"
    );
}

#[test]
fn tauri_bundle_declares_windows_icons() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let config_path = manifest_dir.join("tauri.conf.json");
    let config = read_json(&config_path);

    let icon_entries = config["bundle"]["icon"]
        .as_array()
        .expect("bundle.icon must be an array");

    for icon_path in ["icons/icon.ico", "icons/icon.png"] {
        assert!(
            icon_entries
                .iter()
                .any(|entry| entry.as_str() == Some(icon_path)),
            "bundle.icon must include `{icon_path}`"
        );

        let icon_file = manifest_dir.join(icon_path);
        assert!(
            icon_file.exists(),
            "configured bundle icon path does not exist: {}",
            icon_file.display()
        );
    }
}
