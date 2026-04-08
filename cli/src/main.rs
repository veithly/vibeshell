//! VibeShell CLI - Command-line interface for the VibeShell SSH terminal.
//!
//! This CLI communicates with a VibeShell IPC service to manage SSH
//! sessions and SFTP workflows. The service can run inside the UI or
//! as a headless daemon started by the CLI.

mod commands;
mod daemon;
mod ipc_support;
mod session_alias;
mod terminal;

use std::sync::Arc;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "vshell")]
#[command(
    author,
    version,
    about = "VibeShell - High-performance SSH/SFTP terminal"
)]
#[command(
    long_about = "VibeShell CLI provides terminal-native SSH and SFTP workflows.\n\n\
    Commands talk to a local VibeShell IPC service. If the UI is not open,\n\
    `vshell` can start a headless daemon automatically for SSH/SFTP/session commands.\n\
    By default, `vshell ssh <server>` reuses the earliest active session for that server.\n\
    Pass `--new` only when you explicitly need a fresh SSH session.\n\n\
    Examples:\n\
      vshell ssh prod-web\n\
      vshell ssh prod-web --new\n\
      vshell ssh prod-web -- uname -a\n\
      vshell ssh-session 001 -- journalctl -u nginx -n 200\n\
      vshell sftp prod-web ls /var/www\n\
      vshell sftp prod-web get /etc/nginx/nginx.conf .\\nginx.conf\n\
      vshell sftp prod-web\n\
      vshell sessions\n\
      vshell daemon status"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Show version information
    Version,

    /// Connect to a configured SSH server in the terminal
    #[command(
        alias = "connect",
        long_about = "Connect to a saved server entry and attach the local terminal to its SSH shell.\n\n\
        If no IPC service is running, VibeShell starts a headless daemon automatically.\n\
        By default, this command reuses the earliest active session for the same server.\n\
        Pass `--new` to force creation of a fresh session.\n\
        The target server must already exist in the VibeShell database.\n\n\
        Examples:\n\
          vshell ssh prod-web\n\
          vshell ssh prod-web --new\n\
          vshell ssh prod-web -- hostname\n\
          vshell ssh prod-web -- systemctl status nginx"
    )]
    Ssh(SshArgs),

    /// Interact with an existing SSH session by its short alias ID
    #[command(
        alias = "ss",
        long_about = "Interact with a persistent SSH session using its short alias ID.\n\n\
        When you connect via `vshell ssh <server>`, the earliest active session is reused by default\n\
        and a 3-digit alias (e.g. 001) is assigned automatically. Use `--new` on the original\n\
        `vshell ssh` command only when you need another parallel session.\n\
        Use this command to reattach or run commands on that\n\
        session without needing the full UUID.\n\n\
        Examples:\n\
          vshell ssh-session 001                    # Reattach interactively\n\
          vshell ssh-session 001 -- uname -a        # Execute a single command\n\
          vshell ssh-session 001 -- tail -f app.log # Stream remote output"
    )]
    SshSession(SshSessionArgs),

    /// Open an interactive terminal SFTP session for a configured SSH server
    #[command(
        long_about = "Open a terminal SFTP workflow backed by the VibeShell IPC service.\n\n\
        You can either run a single SFTP command directly:\n\
          vshell sftp prod-web ls /var/www\n\
          vshell sftp prod-web get /remote/file ./local-file\n\
          vshell sftp prod-web put ./local-file /remote/file\n\n\
        Or start the interactive prompt:\n\
          vshell sftp prod-web\n\n\
        Supported commands inside the prompt or direct mode:\n\
          pwd, ls [path], cd <path>, get <remote> [local], put <local> [remote],\n\
          cat <path>, mkdir <path>, rm <path>, mv <old> <new>, help, quit"
    )]
    Sftp(SftpArgs),

    /// List all configured servers
    #[command(alias = "server-list")]
    Servers,

    /// List all active reusable sessions
    #[command(alias = "ls")]
    Sessions,

    /// Attach to an existing session by alias or UUID
    Attach(AttachArgs),

    /// Execute a command on an existing SSH session by alias or UUID
    Exec(ExecArgs),

    /// Kill/terminate session(s)
    Kill(KillArgs),

    /// Start skill server for AI tool integration
    #[command(alias = "mcp-server")]
    SkillServer(SkillServerArgs),

    /// Manage the headless background service used by terminal workflows
    #[command(long_about = "Inspect or start the VibeShell headless daemon.\n\n\
        SSH, SFTP, session listing, attach, and kill commands can auto-start this service,\n\
        but you can also manage it explicitly with `vshell daemon start` and `vshell daemon status`.")]
    Daemon(DaemonArgs),

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

    /// Keep retrying connection on failure (useful for Tailscale/VPN that need browser auth)
    #[arg(long)]
    wait: bool,

    /// Force creation of a brand-new session instead of reusing the earliest active one
    #[arg(long)]
    new: bool,

    /// Run a single remote command instead of opening an interactive shell
    #[arg(last = true)]
    command: Vec<String>,
}

#[derive(Args)]
struct SshSessionArgs {
    /// Session alias ID (e.g. 001, 002)
    alias: String,

    /// Command to execute on the session (omit for interactive mode)
    #[arg(last = true)]
    command: Vec<String>,
}

#[derive(Args)]
struct AttachArgs {
    /// Session alias or UUID to attach to
    session_id: String,
}

#[derive(Args)]
struct ExecArgs {
    /// Session ID to execute the command on
    session_id: String,

    /// Command to execute
    #[arg(last = true)]
    command: Vec<String>,
}

#[derive(Args)]
struct SftpArgs {
    /// Name of the configured server to connect to (not needed with --session)
    #[arg(required_unless_present = "session")]
    server: Option<String>,

    /// Attach to an existing session by ID instead of creating a new one
    #[arg(long)]
    session: Option<String>,

    /// Run a single SFTP operation directly instead of entering the interactive prompt
    #[arg(trailing_var_arg = true)]
    args: Vec<String>,
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
    /// Use stdio transport (stdin/stdout JSON-RPC) — required by Claude Code, Codex, Cursor
    #[arg(long)]
    stdio: bool,

    /// Port to listen on (HTTP mode, ignored when --stdio is set)
    #[arg(long, default_value = "3000")]
    port: u16,
}

#[derive(Args)]
struct DaemonArgs {
    #[command(subcommand)]
    command: DaemonCommand,
}

#[derive(Subcommand)]
enum DaemonCommand {
    /// Start the headless daemon in the background
    Start,
    /// Run the headless daemon in the foreground
    Run,
    /// Report whether the headless daemon socket is reachable
    Status,
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
            commands::ssh::connect(&args.server, &args.command, args.wait, args.new)
        }
        Some(Commands::SshSession(args)) => {
            commands::session::ssh_session(&args.alias, &args.command)
        }
        Some(Commands::Sftp(args)) => {
            commands::sftp::connect(args.server.as_deref(), args.session.as_deref(), &args.args)
        }
        Some(Commands::Servers) => commands::server::list(),
        Some(Commands::Sessions) => commands::session::list(),
        Some(Commands::Attach(args)) => {
            let resolved =
                session_alias::resolve(&args.session_id).unwrap_or_else(|| args.session_id.clone());
            commands::session::attach(&resolved)
        }
        Some(Commands::Exec(args)) => {
            if args.command.is_empty() {
                eprintln!("Error: Please specify a command to execute.");
                eprintln!("Usage: vshell exec <session-id> -- <command>");
                std::process::exit(1);
            }
            let resolved =
                session_alias::resolve(&args.session_id).unwrap_or_else(|| args.session_id.clone());
            commands::session::exec(&resolved, &args.command)
        }
        Some(Commands::Kill(args)) => {
            if args.all {
                commands::session::kill_all()
            } else if let Some(ref id) = args.session_id {
                let resolved = session_alias::resolve(id).unwrap_or_else(|| id.clone());
                commands::session::kill(&resolved)
            } else {
                eprintln!("Error: Please specify a session ID or use --all to kill all sessions.");
                eprintln!("Run 'vshell kill --help' for more information.");
                std::process::exit(1);
            }
        }
        Some(Commands::SkillServer(args)) => {
            if args.stdio {
                run_skill_server_stdio()
            } else {
                run_skill_server(args.port)
            }
        }
        Some(Commands::Daemon(args)) => match args.command {
            DaemonCommand::Start => daemon::start_background(),
            DaemonCommand::Run => daemon::run_foreground(),
            DaemonCommand::Status => {
                daemon::print_status();
                Ok(())
            }
        },
        Some(Commands::Tools) => commands::install::list_tools(),
        Some(Commands::Install(args)) => commands::install::install(&args.tool),
        Some(Commands::Uninstall(args)) => commands::install::uninstall(&args.tool),
        None => {
            println!("VibeShell - High-performance SSH/SFTP terminal");
            println!();
            println!("Run 'vshell --help' for usage information.");
            Ok(())
        }
    }
}

/// Run the skill server on the specified port (HTTP mode)
fn run_skill_server(port: u16) -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;

    rt.block_on(async {
        let database =
            Arc::new(vibeshell_core::Database::new().expect("Failed to initialize database"));
        let session_manager = Arc::new(vibeshell_core::SessionManager::new(database.clone()));

        let server = vibeshell_core::McpServer::new(database, session_manager);

        eprintln!("Starting VibeShell Skill Server (HTTP)...");
        eprintln!("Use Ctrl+C to stop the server.");

        server.run(port).await
    })?;

    Ok(())
}

/// Run the skill server in stdio mode (stdin/stdout JSON-RPC).
///
/// This is the standard transport for Claude Code, Codex CLI, Cursor, etc.
/// The server reads JSON-RPC requests from stdin and writes responses to stdout.
fn run_skill_server_stdio() -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;

    rt.block_on(async {
        let database =
            Arc::new(vibeshell_core::Database::new().expect("Failed to initialize database"));
        let session_manager = Arc::new(vibeshell_core::SessionManager::new(database.clone()));

        vibeshell_core::mcp::stdio::run_stdio(database, session_manager).await
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::Parser;

    #[test]
    fn parses_servers_command() {
        let parsed = Cli::try_parse_from(["vshell", "servers"]);
        assert!(parsed.is_ok(), "vshell servers should parse");
    }

    #[test]
    fn parses_daemon_start_command() {
        let parsed = Cli::try_parse_from(["vshell", "daemon", "start"]);
        assert!(parsed.is_ok(), "vshell daemon start should parse");
    }

    #[test]
    fn parses_sftp_command() {
        let parsed = Cli::try_parse_from(["vshell", "sftp", "example"]);
        assert!(parsed.is_ok(), "vshell sftp <server> should parse");
    }

    #[test]
    fn parses_ssh_single_command() {
        let parsed = Cli::try_parse_from(["vshell", "ssh", "example", "--", "hostname"]);
        assert!(
            parsed.is_ok(),
            "vshell ssh <server> -- <command> should parse"
        );
    }

    #[test]
    fn parses_ssh_with_new_flag() {
        let parsed = Cli::try_parse_from(["vshell", "ssh", "example", "--new"]);
        assert!(parsed.is_ok(), "vshell ssh <server> --new should parse");
    }

    #[test]
    fn parses_sftp_direct_command() {
        let parsed = Cli::try_parse_from(["vshell", "sftp", "example", "ls", "/tmp"]);
        assert!(
            parsed.is_ok(),
            "vshell sftp <server> <command> should parse"
        );
    }

    #[test]
    fn parses_sftp_session_command() {
        let parsed = Cli::try_parse_from(["vshell", "sftp", "--session", "abc-123"]);
        assert!(parsed.is_ok(), "vshell sftp --session <id> should parse");
    }

    #[test]
    fn parses_exec_command() {
        let parsed = Cli::try_parse_from(["vshell", "exec", "abc-123", "--", "hostname"]);
        assert!(
            parsed.is_ok(),
            "vshell exec <session-id> -- <command> should parse"
        );
    }

    #[test]
    fn parses_ssh_session_interactive() {
        let parsed = Cli::try_parse_from(["vshell", "ssh-session", "001"]);
        assert!(parsed.is_ok(), "vshell ssh-session <alias> should parse");
    }

    #[test]
    fn parses_ssh_session_with_command() {
        let parsed = Cli::try_parse_from(["vshell", "ssh-session", "001", "--", "uname", "-a"]);
        assert!(
            parsed.is_ok(),
            "vshell ssh-session <alias> -- <cmd> should parse"
        );
    }

    #[test]
    fn parses_ssh_session_alias_ss() {
        let parsed = Cli::try_parse_from(["vshell", "ss", "001"]);
        assert!(
            parsed.is_ok(),
            "vshell ss <alias> should parse (alias for ssh-session)"
        );
    }
}
