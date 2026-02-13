//! CLI commands for AI tool skill installation management.
//!
//! This module provides commands to list, install, and uninstall
//! VibeShell skill configuration to various AI coding tools.

use anyhow::Result;

/// List all detected AI tools and their installation status.
pub fn list_tools() -> Result<()> {
    let tools = vibeshell_core::install::detect_ai_tools();

    if tools.is_empty() {
        println!("No AI tools detected.");
        return Ok(());
    }

    println!("Detected AI Tools:");
    println!("{:-<60}", "");
    println!(
        "{:<15} {:<15} {:<20} {}",
        "ID", "NAME", "INSTALLED", "VIBESHELL"
    );
    println!("{:-<60}", "");

    for tool in &tools {
        let installed_status = if tool.installed {
            "Yes"
        } else {
            "No"
        };

        let vibeshell_status = if tool.vibeshell_installed {
            "Configured"
        } else if tool.installed {
            "Not configured"
        } else {
            "-"
        };

        println!(
            "{:<15} {:<15} {:<20} {}",
            tool.id, tool.name, installed_status, vibeshell_status
        );
    }

    println!("{:-<60}", "");
    println!();
    println!("Use 'vshell install <tool-id>' to install VibeShell skill.");
    println!("Use 'vshell install all' to install to all detected tools.");

    Ok(())
}

/// Install VibeShell skill to a specific tool or all tools.
pub fn install(tool: &str) -> Result<()> {
    if tool == "all" {
        println!("Installing VibeShell skill to all detected tools...");
        println!();

        let results = vibeshell_core::install::install_to_all();

        if results.is_empty() {
            println!("No AI tools detected.");
            println!("Make sure at least one of these tools is installed:");
            println!("  - Claude Code");
            println!("  - Cursor");
            println!("  - Codex");
            println!("  - Open Code");
            println!("  - Gemini CLI");
            println!("  - OpenClaw");
            return Ok(());
        }

        let mut success_count = 0;
        let mut failure_count = 0;

        for result in results {
            if result.success {
                success_count += 1;
                println!("[OK] {} - VibeShell skill installed successfully", result.tool.name);
                if let Some(backup) = result.backup_path {
                    println!("     Backup: {:?}", backup);
                }
            } else {
                failure_count += 1;
                println!(
                    "[FAIL] {} - {}",
                    result.tool.name,
                    result.error.unwrap_or_else(|| "Unknown error".to_string())
                );
            }
        }

        println!();
        println!(
            "Installation complete: {} succeeded, {} failed",
            success_count, failure_count
        );
    } else {
        // Install to specific tool
        println!("Installing VibeShell skill to {}...", tool);
        let result = vibeshell_core::install::install_by_id(tool)?;

        if result.success {
            println!("VibeShell skill installed successfully to {}.", result.tool.name);
            println!("Config file: {:?}", result.tool.config_path);
            if let Some(backup) = result.backup_path {
                println!("Backup created: {:?}", backup);
            }
            println!();
            println!("VibeShell skill is now available in {}.", result.tool.name);
            println!("Restart {} to load the new configuration.", result.tool.name);
        } else {
            println!(
                "Failed to install VibeShell skill to {}: {}",
                result.tool.name,
                result.error.unwrap_or_else(|| "Unknown error".to_string())
            );
            std::process::exit(1);
        }
    }

    Ok(())
}

/// Uninstall VibeShell skill from a specific tool or all tools.
pub fn uninstall(tool: &str) -> Result<()> {
    if tool == "all" {
        println!("Uninstalling VibeShell skill from all configured tools...");
        println!();

        let results = vibeshell_core::install::uninstall_from_all();

        if results.is_empty() {
            println!("VibeShell skill is not installed in any tool.");
            return Ok(());
        }

        let mut success_count = 0;
        let mut failure_count = 0;

        for result in results {
            if result.success {
                success_count += 1;
                println!("[OK] {} - VibeShell skill uninstalled successfully", result.tool.name);
                if let Some(backup) = result.backup_path {
                    println!("     Backup: {:?}", backup);
                }
            } else {
                failure_count += 1;
                println!(
                    "[FAIL] {} - {}",
                    result.tool.name,
                    result.error.unwrap_or_else(|| "Unknown error".to_string())
                );
            }
        }

        println!();
        println!(
            "Uninstallation complete: {} succeeded, {} failed",
            success_count, failure_count
        );
    } else {
        // Uninstall from specific tool
        println!("Uninstalling VibeShell skill from {}...", tool);
        let result = vibeshell_core::install::uninstall_by_id(tool)?;

        if result.success {
            println!(
                "VibeShell skill uninstalled successfully from {}.",
                result.tool.name
            );
            if let Some(backup) = result.backup_path {
                println!("Backup created: {:?}", backup);
            }
            println!();
            println!("Restart {} to apply the changes.", result.tool.name);
        } else {
            println!(
                "Failed to uninstall VibeShell skill from {}: {}",
                result.tool.name,
                result.error.unwrap_or_else(|| "Unknown error".to_string())
            );
            std::process::exit(1);
        }
    }

    Ok(())
}
