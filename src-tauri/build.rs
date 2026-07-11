use std::{env, path::PathBuf, process::Command};

fn main() {
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

    tauri_build::build()
}
