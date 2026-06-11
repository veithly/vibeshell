use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-env-changed=STATIC_VCRUNTIME");

    if env::var("STATIC_VCRUNTIME").is_ok_and(|value| value.eq_ignore_ascii_case("true")) {
        println!(
            "cargo:warning=Ignoring STATIC_VCRUNTIME=true for VibeShell because tauri-build's static CRT override breaks the shared vibeshell_core -> vshell CLI link on MSVC."
        );
        env::remove_var("STATIC_VCRUNTIME");
    }

    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("Missing CARGO_MANIFEST_DIR"));
    let project_root = manifest_dir.parent().expect("Missing workspace root");
    let dist_index = project_root.join("dist").join("index.html");

    if !dist_index.exists() {
        println!(
            "cargo:warning=Frontend dist not found at {}. Building frontend...",
            dist_index.display()
        );

        // On Windows, npm is invoked as npm.cmd
        let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };

        let status = Command::new(npm)
            .args(["run", "build"])
            .current_dir(project_root)
            .status();

        match status {
            Ok(s) if s.success() => {}
            Ok(s) => panic!("Frontend build failed with exit code: {:?}", s.code()),
            Err(e) => {
                // In CI or check-only builds, the frontend may already be built
                // by a previous step, or we're just running cargo check.
                // Don't panic, just warn.
                println!(
                    "cargo:warning=Could not run '{}': {}. \
                     If running cargo check, this is OK — frontend is built separately.",
                    npm, e
                );
            }
        }
    }

    // Ensure externalBin sidecar binary exists.
    // During checks and while scripts/build-vshell-sidecar.js is building the CLI,
    // the real binary may not exist yet. In those cases we create a placeholder
    // so tauri_build validation can continue. Release app builds should reach
    // this point only after beforeBuildCommand has copied the real CLI here.
    let target = env::var("TARGET").unwrap_or_default();
    let ext = if target.contains("windows") {
        ".exe"
    } else {
        ""
    };
    let binaries_dir = manifest_dir.join("binaries");
    let sidecar_path = binaries_dir.join(format!("vshell-{}{}", target, ext));

    let profile = env::var("PROFILE").unwrap_or_default();
    let building_sidecar = env::var("VIBESHELL_BUILDING_SIDECAR").is_ok();
    let sidecar_missing_or_empty = sidecar_path
        .metadata()
        .map(|metadata| metadata.len() == 0)
        .unwrap_or(true);

    if profile == "release" && sidecar_missing_or_empty && !building_sidecar {
        panic!(
            "Missing real vshell sidecar at {}. Run `node scripts/build-vshell-sidecar.js` from the workspace root before building the Tauri app.",
            sidecar_path.display()
        );
    }

    if !sidecar_path.exists() {
        fs::create_dir_all(&binaries_dir).ok();
        // Create a minimal stub file so tauri_build validation passes
        fs::write(&sidecar_path, b"").ok();
        println!(
            "cargo:warning=Created stub sidecar at {} (will be replaced by real binary for release builds)",
            sidecar_path.display()
        );
    }

    println!("cargo:rerun-if-changed={}", sidecar_path.display());

    tauri_build::build()
}
