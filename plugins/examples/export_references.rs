//! Generate documentation without opening an application database or user directory.
use std::{env, error::Error, fs, path::Path};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args().skip(1).collect();
    let check = match args.as_slice() {
        [] => false,
        [arg] if arg == "--check" => true,
        _ => return Err("Usage: export_references [--check]".into()),
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Missing repository root")?;
    let catalog = vibeshell_plugins::builtin_catalog()?;
    for directory in [
        "skills/vibeshell",
        ".claude/skills/vibeshell",
        ".codex/skills/vibeshell",
    ] {
        let references = root.join(directory).join("references");
        if !check {
            fs::create_dir_all(&references)?;
        }
        for manifest in &catalog {
            let path = references.join(format!("{}.md", manifest.id));
            let expected = vibeshell_plugins::agent_reference(manifest)?;
            if check {
                if fs::read_to_string(&path)? != expected {
                    return Err(format!("Stale reference: {}", path.display()).into());
                }
            } else if fs::read_to_string(&path).ok().as_deref() != Some(&expected) {
                fs::write(&path, &expected)?;
            }
        }
    }
    println!(
        "{} references for {} plugins in three repository Skill roots",
        if check { "Verified" } else { "Generated" },
        catalog.len()
    );
    Ok(())
}
