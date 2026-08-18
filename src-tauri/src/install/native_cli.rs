//! Install the CLI sidecar bundled with the desktop application into a stable
//! user-level command location.

#[cfg(unix)]
use std::path::Path;
use std::path::PathBuf;

#[cfg(unix)]
use anyhow::Context;
use anyhow::Result;

/// Copy the bundled native CLI to a persistent user command directory.
///
/// Windows installers add the application directory to the user PATH through
/// NSIS/WiX hooks, so no duplicate copy is needed there. macOS DMGs and Linux
/// AppImages cannot reliably modify a system PATH during installation; for
/// those platforms we persist the sidecar at `~/.local/bin/vibeshell`.
#[cfg(windows)]
pub fn install_bundled_cli_for_user() -> Result<Option<PathBuf>> {
    Ok(None)
}

#[cfg(unix)]
pub fn install_bundled_cli_for_user() -> Result<Option<PathBuf>> {
    let Some(source) = find_bundled_cli()? else {
        // Expected during `cargo test`, `cargo run`, and desktop development
        // when a release sidecar has not been prepared.
        return Ok(None);
    };
    let home = directories::BaseDirs::new()
        .context("Could not determine the current user's home directory")?;
    let bin_dir = home.home_dir().join(".local").join("bin");
    std::fs::create_dir_all(&bin_dir)
        .with_context(|| format!("Failed to create CLI directory {}", bin_dir.display()))?;
    let destination = bin_dir.join("vibeshell");

    if files_match(&source, &destination)? {
        ensure_executable(&destination)?;
        return Ok(Some(destination));
    }

    let temporary = bin_dir.join(format!(".vibeshell.install-{}", std::process::id()));
    if temporary.exists() {
        let _ = std::fs::remove_file(&temporary);
    }
    std::fs::copy(&source, &temporary).with_context(|| {
        format!(
            "Failed to copy bundled CLI from {} to {}",
            source.display(),
            temporary.display()
        )
    })?;
    ensure_executable(&temporary)?;
    std::fs::rename(&temporary, &destination)
        .with_context(|| format!("Failed to activate native CLI at {}", destination.display()))?;

    if !path_contains(&bin_dir) {
        log::info!(
            "[Install] Native CLI installed at {}; ~/.local/bin is not currently in PATH",
            destination.display()
        );
    } else {
        log::info!(
            "[Install] Native CLI installed at {}",
            destination.display()
        );
    }
    Ok(Some(destination))
}

#[cfg(unix)]
fn find_bundled_cli() -> Result<Option<PathBuf>> {
    let current_exe = std::env::current_exe().context("Could not locate the desktop executable")?;
    let Some(executable_dir) = current_exe.parent() else {
        return Ok(None);
    };

    let mut candidates = vec![executable_dir.join("vibeshell")];
    if let Some(contents_dir) = executable_dir.parent() {
        candidates.push(contents_dir.join("Resources").join("vibeshell"));
    }

    // Keep a suffix-aware fallback for development bundles and future Tauri
    // layout changes, while explicitly excluding the desktop executable.
    if let Ok(entries) = std::fs::read_dir(executable_dir) {
        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let file_name = file_name.to_string_lossy();
            if file_name.starts_with("vibeshell-") && !file_name.contains("desktop") {
                candidates.push(entry.path());
            }
        }
    }

    let current_canonical = current_exe.canonicalize().unwrap_or(current_exe);
    for candidate in candidates {
        if !candidate.is_file() {
            continue;
        }
        let canonical = candidate
            .canonicalize()
            .unwrap_or_else(|_| candidate.clone());
        if canonical != current_canonical {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

#[cfg(unix)]
fn files_match(left: &Path, right: &Path) -> Result<bool> {
    if !right.is_file() {
        return Ok(false);
    }
    let left_metadata = std::fs::metadata(left)?;
    let right_metadata = std::fs::metadata(right)?;
    if left_metadata.len() != right_metadata.len() {
        return Ok(false);
    }
    Ok(std::fs::read(left)? == std::fs::read(right)?)
}

#[cfg(unix)]
fn ensure_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = std::fs::metadata(path)?.permissions();
    permissions.set_mode(permissions.mode() | 0o755);
    std::fs::set_permissions(path, permissions)
        .with_context(|| format!("Failed to mark {} executable", path.display()))
}

#[cfg(unix)]
fn path_contains(directory: &Path) -> bool {
    std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).any(|entry| entry == directory))
        .unwrap_or(false)
}
