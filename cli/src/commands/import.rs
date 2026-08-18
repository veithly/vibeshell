//! Import SSH connection profiles into the shared VibeShell database.

use std::path::PathBuf;

use anyhow::{Context, Result};
use vibeshell_core::ssh_import::{
    import_preview, preview_import, ImportPreview, ImportReport, ImportSourceKind,
};

pub fn run(
    source: ImportSourceKind,
    path: Option<PathBuf>,
    dry_run: bool,
    json: bool,
) -> Result<()> {
    let preview = preview_import(source, path)?;

    if dry_run {
        if json {
            println!("{}", serde_json::to_string_pretty(&preview)?);
        } else {
            print_preview(&preview);
        }
        return Ok(());
    }

    let database = vibeshell_core::Database::new().context("Failed to open VibeShell database")?;
    let report = import_preview(&database, &preview)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_report(&report);
    }
    Ok(())
}

fn print_preview(preview: &ImportPreview) {
    println!("SSH configuration import preview");
    println!("{:-<88}", "");
    if preview.servers.is_empty() {
        println!("No importable SSH profiles found.");
    } else {
        println!(
            "{:<10} {:<24} {:<22} {:<6} USER",
            "SOURCE", "NAME", "HOST", "PORT"
        );
        println!("{:-<88}", "");
        for server in &preview.servers {
            println!(
                "{:<10} {:<24} {:<22} {:<6} {}",
                server.source.label(),
                server.name,
                server.host,
                server.port,
                server.username
            );
        }
    }
    println!("{:-<88}", "");
    println!("{} profile(s) would be imported.", preview.servers.len());
    print_warnings(&preview.warnings);
}

fn print_report(report: &ImportReport) {
    println!(
        "Import complete: {} imported, {} skipped, {} renamed ({} discovered).",
        report.imported, report.skipped, report.renamed, report.discovered
    );
    for server in &report.servers {
        let marker = match server.status.as_str() {
            "skipped" => "SKIP",
            "imported_renamed" => "ADD+RENAME",
            _ => "ADD",
        };
        println!(
            "[{marker}] {:<10} {} -> {}",
            server.source.label(),
            server.source_name,
            server.name
        );
        if let Some(message) = server.message.as_deref() {
            println!("       {message}");
        }
    }
    print_warnings(&report.warnings);
}

fn print_warnings(warnings: &[String]) {
    if warnings.is_empty() {
        return;
    }
    eprintln!();
    eprintln!("Warnings:");
    for warning in warnings {
        eprintln!("  - {warning}");
    }
}
