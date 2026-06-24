/// Return the current application version from Cargo package metadata.
#[tauri::command]
pub fn get_app_version() -> String {
    crate::version().to_string()
}

fn validate_external_url(url: &str) -> Result<&str, String> {
    let trimmed = url.trim();
    if trimmed.starts_with("https://") || trimmed.starts_with("http://") {
        Ok(trimmed)
    } else {
        Err("Only http(s) URLs can be opened".to_string())
    }
}

#[cfg(target_os = "macos")]
fn open_url_with_system(url: &str) -> std::io::Result<std::process::ExitStatus> {
    std::process::Command::new("open").arg(url).status()
}

#[cfg(target_os = "windows")]
fn open_url_with_system(url: &str) -> std::io::Result<std::process::ExitStatus> {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x08000000;
    std::process::Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", url])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
fn open_url_with_system(url: &str) -> std::io::Result<std::process::ExitStatus> {
    std::process::Command::new("xdg-open").arg(url).status()
}

/// Open an external http(s) URL using the system default handler.
#[tauri::command]
pub fn open_external_url(url: String) -> Result<(), String> {
    let url = validate_external_url(&url)?;
    let status = open_url_with_system(url).map_err(|e| format!("Failed to open URL: {}", e))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("System URL opener exited with {}", status))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_version_uses_package_metadata() {
        assert_eq!(get_app_version(), crate::version());
    }

    #[test]
    fn external_url_validation_allows_http_urls() {
        assert_eq!(
            validate_external_url(" https://github.com/veithly/vibeshell ").unwrap(),
            "https://github.com/veithly/vibeshell"
        );
        assert!(validate_external_url("http://localhost:1420").is_ok());
    }

    #[test]
    fn external_url_validation_rejects_non_http_urls() {
        assert!(validate_external_url("file:///etc/passwd").is_err());
        assert!(validate_external_url("javascript:alert(1)").is_err());
    }
}
