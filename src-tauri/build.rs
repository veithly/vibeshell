use std::{
    env,
    path::PathBuf,
    process::Command,
};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("Missing CARGO_MANIFEST_DIR"));
    let project_root = manifest_dir
        .parent()
        .expect("Missing workspace root");
    let dist_index = project_root.join("dist").join("index.html");

    if !dist_index.exists() {
        println!(
            "cargo:warning=Frontend dist not found at {}. Building frontend...",
            dist_index.display()
        );
        let status = Command::new("npm")
            .args(["run", "build"])
            .current_dir(project_root)
            .status()
            .expect("Failed to run npm build (is Node.js installed?)");
        if !status.success() {
            panic!("Frontend build failed. Fix the errors above and rebuild.");
        }
    }

    tauri_build::build()
}
