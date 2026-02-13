//! VibeShell CLI - Command-line interface for the VibeShell SSH terminal.
//!
//! This CLI communicates with the VibeShell GUI application via IPC
//! to manage SSH sessions and connections.

mod commands;

use std::sync::Arc;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "vshell")]
#[command(author, version, about = "VibeShell - High-performance SSH/SFTP terminal")]
#[command(long_about = "VibeShell CLI provides command-line access to SSH session management.\n\n\
    The CLI communicates with the VibeShell GUI application via IPC.\n\
    Make sure the GUI is running before using session commands.")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Show version information
    Version,

    /// Connect to a configured SSH server
    #[command(alias = "connect")]
    Ssh(SshArgs),

    /// List all active sessions
    #[command(alias = "ls")]
    Sessions,

    /// Attach to an existing session
    Attach(AttachArgs),

    /// Kill/terminate session(s)
    Kill(KillArgs),

    /// Start skill server for AI tool integration
    #[command(alias = "mcp-server")]
    SkillServer(SkillServerArgs),

    /// List detected AI tools and their skill installation status
    Tools,

    /// Install VibeShell skill to an AI tool
    Install(InstallArgs),

    /// Uninstall VibeShell skill from an AI tool
    Uninstall(UninstallArgs),
}

#[derive(Args)]
struct SshArgs {
    /// Name of the configured server to connect to
    server: String,
}

#[derive(Args)]
struct AttachArgs {
    /// ID of the session to attach to
    session_id: String,
}

#[derive(Args)]
struct KillArgs {
    /// ID of the session to kill (omit to use --all)
    session_id: Option<String>,

    /// Kill all active sessions
    #[arg(long, short)]
    all: bool,
}

#[derive(Args)]
struct SkillServerArgs {
    /// Port to listen on
    #[arg(long, default_value = "3000")]
    port: u16,
}

#[derive(Args)]
struct InstallArgs {
    /// Tool name (claude-code, cursor, codex, opencode) or "all"
    tool: String,
}

#[derive(Args)]
struct UninstallArgs {
    /// Tool name (claude-code, cursor, codex, opencode) or "all"
    tool: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Version) => {
            println!("vshell {}", vibeshell_core::version());
            Ok(())
        }
        Some(Commands::Ssh(args)) => {
            commands::ssh::connect(&args.server)
        }
        Some(Commands::Sessions) => {
            commands::session::list()
        }
        Some(Commands::Attach(args)) => {
            commands::session::attach(&args.session_id)
        }
        Some(Commands::Kill(args)) => {
            if args.all {
                commands::session::kill_all()
            } else if let Some(session_id) = args.session_id {
                commands::session::kill(&session_id)
            } else {
                eprintln!("Error: Please specify a session ID or use --all to kill all sessions.");
                eprintln!("Run 'vshell kill --help' for more information.");
                std::process::exit(1);
            }
        }
        Some(Commands::SkillServer(args)) => {
            run_skill_server(args.port)
        }
        Some(Commands::Tools) => {
            commands::install::list_tools()
        }
        Some(Commands::Install(args)) => {
            commands::install::install(&args.tool)
        }
        Some(Commands::Uninstall(args)) => {
            commands::install::uninstall(&args.tool)
        }
        None => {
            println!("VibeShell - High-performance SSH/SFTP terminal");
            println!();
            println!("Run 'vshell --help' for usage information.");
            Ok(())
        }
    }
}

/// Run the skill server on the specified port
fn run_skill_server(port: u16) -> Result<()> {
    // Create a tokio runtime for the async skill server
    let rt = tokio::runtime::Runtime::new()?;

    rt.block_on(async {
        // Initialize database
        let database = Arc::new(
            vibeshell_core::Database::new()
                .expect("Failed to initialize database")
        );

        // Initialize session manager
        let session_manager = Arc::new(
            vibeshell_core::SessionManager::new(database.clone())
        );

        // Create and run skill server (MCP protocol)
        let server = vibeshell_core::McpServer::new(database, session_manager);

        println!("Starting VibeShell Skill Server...");
        println!("Use Ctrl+C to stop the server.");

        server.run(port).await
    })?;

    Ok(())
}
