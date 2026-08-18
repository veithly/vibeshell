//! VibeShell CLI - Command-line interface for the VibeShell SSH terminal.
//!
//! This CLI communicates with a VibeShell IPC service to manage SSH
//! sessions and SFTP workflows. The service can run inside the UI or
//! as a headless daemon started by the CLI.

mod command_input;
mod commands;
mod daemon;
mod ipc_support;
mod session_alias;
mod terminal;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Args, Parser, Subcommand, ValueEnum};
use command_input::CommandInputArgs;
use commands::file_tools::ContentInputArgs;

#[derive(Parser)]
#[command(name = "vibeshell")]
#[command(
    author,
    version,
    about = "VibeShell - High-performance SSH/SFTP terminal"
)]
#[command(
    long_about = "VibeShell CLI provides terminal-native SSH and SFTP workflows.\n\n\
    Commands talk to a local VibeShell IPC service. If the UI is not open,\n\
    `vibeshell` can start a headless daemon automatically for SSH/SFTP/session commands.\n\
    By default, `vibeshell ssh <server>` reuses the earliest active session for that server.\n\
    Pass `--new` only when you explicitly need a fresh SSH session.\n\
    If your local shell mangles nested quotes, prefer `--command-file` or `--command-stdin`\n\
    instead of stacking more escaping.\n\n\
    Examples:\n\
      vibeshell ssh prod-web\n\
      vibeshell ssh prod-web --new\n\
      vibeshell ssh prod-web -- uname -a\n\
      vibeshell ssh prod-web --command-file .\\remote-command.sh\n\
      vibeshell ssh-session 001 -- journalctl -u nginx -n 200\n\
      vibeshell sftp prod-web ls /var/www\n\
      vibeshell sftp prod-web get /etc/nginx/nginx.conf .\\nginx.conf\n\
      vibeshell sftp prod-web\n\
      vibeshell sessions\n\
      vibeshell daemon status"
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
        For heavily quoted commands, use `--command-file <path>` or pipe the command into\n\
        `--command-stdin` to bypass local shell parsing.\n\
        The target server must already exist in the VibeShell database.\n\n\
        Examples:\n\
          vibeshell ssh prod-web\n\
          vibeshell ssh prod-web --new\n\
          vibeshell ssh prod-web -- hostname\n\
          vibeshell ssh prod-web --command-file .\\remote-command.sh\n\
          vibeshell ssh prod-web -- systemctl status nginx"
    )]
    Ssh(SshArgs),

    /// Interact with an existing SSH session by its short alias ID
    #[command(
        alias = "ss",
        long_about = "Interact with a persistent SSH session using its short alias ID.\n\n\
        When you connect via `vibeshell ssh <server>`, the earliest active session is reused by default\n\
        and a 3-digit alias (e.g. 001) is assigned automatically. Use `--new` on the original\n\
        `vibeshell ssh` command only when you need another parallel session.\n\
        Use this command to reattach or run commands on that\n\
        session without needing the full UUID. When nested quoting gets messy,\n\
        prefer `--command-file` or `--command-stdin`.\n\n\
        Examples:\n\
          vibeshell ssh-session 001                    # Reattach interactively\n\
          vibeshell ssh-session 001 -- uname -a        # Execute a single command\n\
          vibeshell ssh-session 001 --command-file .\\remote-command.sh\n\
          vibeshell ssh-session 001 -- tail -f app.log # Stream remote output"
    )]
    SshSession(SshSessionArgs),

    /// Open an interactive terminal SFTP session for a configured SSH server
    #[command(
        long_about = "Open a terminal SFTP workflow backed by the VibeShell IPC service.\n\n\
        You can either run a single SFTP command directly:\n\
          vibeshell sftp prod-web ls /var/www\n\
          vibeshell sftp prod-web get /remote/file ./local-file\n\
          vibeshell sftp prod-web put ./local-file /remote/file\n\
          vibeshell sftp prod-web put ./local-folder /remote/parent\n\
          vibeshell sftp prod-web sync ./dist /var/www --delete\n\n\
        Or start the interactive prompt:\n\
          vibeshell sftp prod-web\n\n\
        Supported commands inside the prompt or direct mode:\n\
          pwd, ls [path], cd <path>, get <remote> [local], put <local> [remote],\n\
          sync <local-dir> [remote-dir] [--delete] [--exclude <pattern>] [--no-gitignore],\n\
          cat <path>, mkdir <path>, rm <path>, mv <old> <new>, help, quit"
    )]
    Sftp(SftpArgs),

    /// Search remote files with rg-style output
    #[command(
        alias = "search",
        long_about = "Search remote files using ripgrep when available, with grep fallback.\n\n\
        The target is a configured server name by default. Add --session when the target is\n\
        an existing session alias or UUID.\n\n\
        Examples:\n\
          vibeshell rg prod-web TODO /srv/app\n\
          vibeshell rg prod-web \"listen 80\" /etc/nginx -i\n\
          vibeshell rg --session 001 TODO /srv/app --glob \"*.rs\""
    )]
    Rg(RgArgs),

    /// Read a remote text file
    #[command(
        alias = "read-file",
        alias = "cat-file",
        long_about = "Read a remote text file through SFTP.\n\n\
        The target is a configured server name by default. Add --session when the target is\n\
        an existing session alias or UUID.\n\n\
        Examples:\n\
          vibeshell get-content prod-web /etc/nginx/nginx.conf\n\
          vibeshell get-content --session 001 /var/log/app.log --max-bytes 200000"
    )]
    GetContent(GetContentArgs),

    /// Create a remote text file
    #[command(
        alias = "new-file",
        long_about = "Create a remote text file through SFTP. Fails if the file exists unless\n\
        --overwrite is passed.\n\n\
        Examples:\n\
          vibeshell add-file prod-web /tmp/hello.txt --content \"hello\\n\"\n\
          Get-Content .\\config.yml | vibeshell add-file prod-web /tmp/config.yml --content-stdin\n\
          vibeshell add-file --session 001 /opt/app/.env --content-file .\\.env --parents"
    )]
    AddFile(AddFileArgs),

    /// Edit a remote text file
    #[command(
        alias = "write-file",
        alias = "set-content",
        long_about = "Edit an existing remote text file through SFTP. Use a content source for a\n\
        full-file replacement, or use --replace/--with for exact text replacement.\n\n\
        Examples:\n\
          vibeshell edit-file prod-web /tmp/config.yml --content-file .\\config.yml\n\
          vibeshell edit-file --session 001 /etc/app.conf --replace \"debug=false\" --with \"debug=true\"\n\
          Get-Content .\\app.conf | vibeshell edit-file prod-web /etc/app.conf --content-stdin"
    )]
    EditFile(EditFileArgs),

    /// List all configured servers
    #[command(alias = "server-list")]
    Servers,

    /// Import OpenSSH, PuTTY, and Tabby connection profiles
    #[command(
        long_about = "Discover and import SSH profiles into the shared VibeShell database. Use --dry-run to preview without changing the database. The auto source discovers OpenSSH, PuTTY, and Tabby at their platform-default locations. Third-party passwords are never copied."
    )]
    Import(ImportArgs),

    /// List all active reusable sessions
    #[command(alias = "ls")]
    Sessions,

    /// Attach to an existing session by alias or UUID
    Attach(AttachArgs),

    /// Execute a command on an existing SSH session by alias or UUID
    Exec(ExecArgs),

    /// Send keystrokes to a session (for responding to interactive prompts)
    #[command(
        alias = "sk",
        long_about = "Send keystrokes to an existing session's PTY.\n\n\
        Each argument is either a named key or literal text, sent in order.\n\n\
        Named keys: enter, space, tab, esc, backspace, delete,\n\
                    up, down, left, right, ctrl-c, ctrl-d, ctrl-z\n\n\
        Examples:\n\
          vibeshell send-key 001 y enter          # answer 'y' + Enter\n\
          vibeshell send-key 001 yes enter        # type 'yes' + Enter\n\
          vibeshell send-key 001 ctrl-c           # interrupt running process\n\
          vibeshell send-key 001 space            # press Space (e.g. for pager)"
    )]
    SendKey(SendKeyArgs),

    /// Kill/terminate session(s)
    Kill(KillArgs),

    /// Manage the headless background service used by terminal workflows
    #[command(long_about = "Inspect or start the VibeShell headless daemon.\n\n\
        SSH, SFTP, session listing, attach, and kill commands can auto-start this service,\n\
        but you can also manage it explicitly with `vibeshell daemon start` and `vibeshell daemon status`.")]
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

    #[command(flatten)]
    command_input: CommandInputArgs,
}

#[derive(Args)]
struct SshSessionArgs {
    /// Session alias ID (e.g. 001, 002)
    alias: String,

    #[command(flatten)]
    command_input: CommandInputArgs,
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

    #[command(flatten)]
    command_input: CommandInputArgs,
}

#[derive(Args)]
struct SendKeyArgs {
    /// Session alias or UUID to send keys to
    session_id: String,

    /// Keys to send (named keys like 'enter', 'space', 'ctrl-c', or literal text)
    #[arg(required = true, num_args = 1..)]
    keys: Vec<String>,
}

#[derive(Args)]
struct SftpArgs {
    /// Name of the configured server to connect to (not needed with --session)
    #[arg(required_unless_present = "session")]
    server: Option<String>,

    /// Attach to an existing session by alias or ID instead of creating a new one
    #[arg(long)]
    session: Option<String>,

    /// Run a single SFTP operation directly instead of entering the interactive prompt
    #[arg(trailing_var_arg = true)]
    args: Vec<String>,
}

#[derive(Args)]
struct RemotePathArgs {
    /// Treat target as an existing session alias/UUID instead of a server name
    #[arg(long)]
    session: bool,

    /// Server name, or session alias/UUID when --session is set
    target: String,

    /// Remote file path
    path: String,
}

#[derive(Args)]
struct GetContentArgs {
    #[command(flatten)]
    remote: RemotePathArgs,

    /// Maximum bytes to read before truncating output
    #[arg(long = "max-bytes", default_value_t = 1_048_576)]
    max_bytes: u64,
}

#[derive(Args)]
struct AddFileArgs {
    #[command(flatten)]
    remote: RemotePathArgs,

    #[command(flatten)]
    content: ContentInputArgs,

    /// Replace the file if it already exists
    #[arg(long)]
    overwrite: bool,

    /// Create parent directories if needed
    #[arg(long)]
    parents: bool,
}

#[derive(Args)]
struct EditFileArgs {
    #[command(flatten)]
    remote: RemotePathArgs,

    #[command(flatten)]
    content: ContentInputArgs,

    /// Exact text to replace in the remote file
    #[arg(long = "replace")]
    replace: Option<String>,

    /// Replacement text used with --replace
    #[arg(long = "with")]
    with_text: Option<String>,

    /// Replace every occurrence instead of only the first
    #[arg(long)]
    all: bool,
}

#[derive(Args)]
struct RgArgs {
    /// Treat target as an existing session alias/UUID instead of a server name
    #[arg(long)]
    session: bool,

    /// Server name, or session alias/UUID when --session is set
    target: String,

    /// Pattern to search for
    pattern: String,

    /// Remote directory or file to search
    path: Option<String>,

    /// Case-insensitive search
    #[arg(short = 'i', long = "ignore-case")]
    ignore_case: bool,

    /// Treat the pattern as a literal string
    #[arg(short = 'F', long = "fixed-strings")]
    fixed_strings: bool,

    /// Include hidden files when ripgrep is available
    #[arg(long)]
    hidden: bool,

    /// Include/exclude paths with rg-style glob patterns
    #[arg(short = 'g', long = "glob")]
    globs: Vec<String>,

    /// Maximum output lines to print
    #[arg(long = "max-results", default_value_t = 200)]
    max_results: usize,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ImportSourceArg {
    Auto,
    #[value(name = "openssh", alias = "open-ssh", alias = "ssh")]
    OpenSsh,
    Putty,
    Tabby,
}

impl From<ImportSourceArg> for vibeshell_core::ssh_import::ImportSourceKind {
    fn from(source: ImportSourceArg) -> Self {
        match source {
            ImportSourceArg::Auto => Self::Auto,
            ImportSourceArg::OpenSsh => Self::OpenSsh,
            ImportSourceArg::Putty => Self::Putty,
            ImportSourceArg::Tabby => Self::Tabby,
        }
    }
}

#[derive(Args)]
struct ImportArgs {
    /// Source to import: auto, openssh, putty, or tabby
    #[arg(value_enum, default_value = "auto")]
    source: ImportSourceArg,

    /// Read one explicit config file or PuTTY sessions directory
    #[arg(long)]
    path: Option<PathBuf>,

    /// Preview discovered profiles without changing the database
    #[arg(long)]
    dry_run: bool,

    /// Print machine-readable JSON
    #[arg(long)]
    json: bool,
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
    /// Tool name (claude-code, cursor, codex, opencode, pi) or "all"
    tool: String,
}

#[derive(Args)]
struct UninstallArgs {
    /// Tool name (claude-code, cursor, codex, opencode, pi) or "all"
    tool: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // The native binary carries the canonical Skill. Keep agent directories
    // configured without requiring a separate Node/npm installation step.
    if !matches!(&cli.command, Some(Commands::Uninstall(_)))
        && std::env::var_os("VIBESHELL_SKIP_SKILL_AUTO_INSTALL").is_none()
    {
        let _ = vibeshell_core::install::install_to_all();
    }

    match cli.command {
        Some(Commands::Version) => {
            println!("vibeshell {}", vibeshell_core::version());
            Ok(())
        }
        Some(Commands::Ssh(args)) => {
            let command = args.command_input.resolve()?;
            commands::ssh::connect(&args.server, &command, args.wait, args.new)
        }
        Some(Commands::SshSession(args)) => {
            let command = args.command_input.resolve()?;
            commands::session::ssh_session(&args.alias, &command)
        }
        Some(Commands::Sftp(args)) => {
            let resolved_session = args
                .session
                .as_ref()
                .map(|id| session_alias::resolve(id).unwrap_or_else(|| id.clone()));
            commands::sftp::connect(
                args.server.as_deref(),
                resolved_session.as_deref(),
                &args.args,
            )
        }
        Some(Commands::Rg(args)) => commands::file_tools::rg(
            &args.target,
            args.session,
            &args.pattern,
            args.path.as_deref(),
            args.ignore_case,
            args.fixed_strings,
            args.hidden,
            args.globs,
            args.max_results,
        ),
        Some(Commands::GetContent(args)) => commands::file_tools::get_content(
            &args.remote.target,
            args.remote.session,
            &args.remote.path,
            args.max_bytes,
        ),
        Some(Commands::AddFile(args)) => commands::file_tools::add_file(
            &args.remote.target,
            args.remote.session,
            &args.remote.path,
            &args.content,
            args.overwrite,
            args.parents,
        ),
        Some(Commands::EditFile(args)) => commands::file_tools::edit_file(
            &args.remote.target,
            args.remote.session,
            &args.remote.path,
            &args.content,
            args.replace.as_deref(),
            args.with_text.as_deref(),
            args.all,
        ),
        Some(Commands::Servers) => commands::server::list(),
        Some(Commands::Import(args)) => {
            commands::import::run(args.source.into(), args.path, args.dry_run, args.json)
        }
        Some(Commands::Sessions) => commands::session::list(),
        Some(Commands::Attach(args)) => {
            let resolved =
                session_alias::resolve(&args.session_id).unwrap_or_else(|| args.session_id.clone());
            commands::session::attach(&resolved)
        }
        Some(Commands::Exec(args)) => {
            let command = args.command_input.resolve()?;
            if command.is_empty() {
                eprintln!("Error: Please specify a command to execute.");
                eprintln!("Usage: vibeshell exec <session-id> -- <command>");
                eprintln!("   or: vibeshell exec <session-id> --command-file <path>");
                eprintln!("   or: <command> | vibeshell exec <session-id> --command-stdin");
                std::process::exit(1);
            }
            let resolved =
                session_alias::resolve(&args.session_id).unwrap_or_else(|| args.session_id.clone());
            commands::session::exec(&resolved, &command)
        }
        Some(Commands::SendKey(args)) => {
            let resolved =
                session_alias::resolve(&args.session_id).unwrap_or_else(|| args.session_id.clone());
            commands::session::send_key(&resolved, &args.keys)
        }
        Some(Commands::Kill(args)) => {
            if args.all {
                commands::session::kill_all()
            } else if let Some(ref id) = args.session_id {
                let resolved = session_alias::resolve(id).unwrap_or_else(|| id.clone());
                commands::session::kill(&resolved)
            } else {
                eprintln!("Error: Please specify a session ID or use --all to kill all sessions.");
                eprintln!("Run 'vibeshell kill --help' for more information.");
                std::process::exit(1);
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
            println!("Run 'vibeshell --help' for usage information.");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::Parser;

    #[test]
    fn parses_servers_command() {
        let parsed = Cli::try_parse_from(["vibeshell", "servers"]);
        assert!(parsed.is_ok(), "vibeshell servers should parse");
    }

    #[test]
    fn parses_import_auto_dry_run() {
        let parsed = Cli::try_parse_from(["vibeshell", "import", "auto", "--dry-run", "--json"]);
        assert!(parsed.is_ok(), "vibeshell import auto should parse");
    }

    #[test]
    fn parses_import_explicit_openssh_path() {
        let parsed = Cli::try_parse_from([
            "vibeshell",
            "import",
            "openssh",
            "--path",
            "/tmp/ssh-config",
        ]);
        assert!(
            parsed.is_ok(),
            "vibeshell import openssh --path <path> should parse"
        );
    }

    #[test]
    fn parses_daemon_start_command() {
        let parsed = Cli::try_parse_from(["vibeshell", "daemon", "start"]);
        assert!(parsed.is_ok(), "vibeshell daemon start should parse");
    }

    #[test]
    fn parses_sftp_command() {
        let parsed = Cli::try_parse_from(["vibeshell", "sftp", "example"]);
        assert!(parsed.is_ok(), "vibeshell sftp <server> should parse");
    }

    #[test]
    fn parses_ssh_single_command() {
        let parsed = Cli::try_parse_from(["vibeshell", "ssh", "example", "--", "hostname"]);
        assert!(
            parsed.is_ok(),
            "vibeshell ssh <server> -- <command> should parse"
        );
    }

    #[test]
    fn parses_ssh_command_file() {
        let parsed = Cli::try_parse_from([
            "vibeshell",
            "ssh",
            "example",
            "--command-file",
            ".\\remote-command.sh",
        ]);
        assert!(
            parsed.is_ok(),
            "vibeshell ssh <server> --command-file <path> should parse"
        );
    }

    #[test]
    fn parses_ssh_command_stdin() {
        let parsed = Cli::try_parse_from(["vibeshell", "ssh", "example", "--command-stdin"]);
        assert!(
            parsed.is_ok(),
            "vibeshell ssh <server> --command-stdin should parse"
        );
    }

    #[test]
    fn parses_ssh_with_new_flag() {
        let parsed = Cli::try_parse_from(["vibeshell", "ssh", "example", "--new"]);
        assert!(parsed.is_ok(), "vibeshell ssh <server> --new should parse");
    }

    #[test]
    fn parses_sftp_direct_command() {
        let parsed = Cli::try_parse_from(["vibeshell", "sftp", "example", "ls", "/tmp"]);
        assert!(
            parsed.is_ok(),
            "vibeshell sftp <server> <command> should parse"
        );
    }

    #[test]
    fn parses_sftp_session_command() {
        let parsed = Cli::try_parse_from(["vibeshell", "sftp", "--session", "abc-123"]);
        assert!(parsed.is_ok(), "vibeshell sftp --session <id> should parse");
    }

    #[test]
    fn parses_rg_command() {
        let parsed = Cli::try_parse_from([
            "vibeshell",
            "rg",
            "example",
            "TODO",
            "/srv/app",
            "--glob",
            "*.rs",
            "--max-results",
            "50",
        ]);
        assert!(
            parsed.is_ok(),
            "vibeshell rg <server> <pattern> [path] should parse"
        );
    }

    #[test]
    fn parses_get_content_command() {
        let parsed = Cli::try_parse_from([
            "vibeshell",
            "get-content",
            "--session",
            "001",
            "/etc/nginx/nginx.conf",
        ]);
        assert!(
            parsed.is_ok(),
            "vibeshell get-content --session <id> <path> should parse"
        );
    }

    #[test]
    fn parses_add_file_command() {
        let parsed = Cli::try_parse_from([
            "vibeshell",
            "add-file",
            "example",
            "/tmp/payload.txt",
            "--content",
            "payload",
            "--parents",
        ]);
        assert!(
            parsed.is_ok(),
            "vibeshell add-file <server> <path> --content <text> should parse"
        );
    }

    #[test]
    fn parses_edit_file_replace_command() {
        let parsed = Cli::try_parse_from([
            "vibeshell",
            "edit-file",
            "example",
            "/tmp/payload.txt",
            "--replace",
            "old",
            "--with",
            "new",
            "--all",
        ]);
        assert!(
            parsed.is_ok(),
            "vibeshell edit-file <server> <path> --replace <old> --with <new> should parse"
        );
    }

    #[test]
    fn parses_exec_command() {
        let parsed = Cli::try_parse_from(["vibeshell", "exec", "abc-123", "--", "hostname"]);
        assert!(
            parsed.is_ok(),
            "vibeshell exec <session-id> -- <command> should parse"
        );
    }

    #[test]
    fn parses_exec_command_file() {
        let parsed = Cli::try_parse_from([
            "vibeshell",
            "exec",
            "abc-123",
            "--command-file",
            ".\\remote-command.sh",
        ]);
        assert!(
            parsed.is_ok(),
            "vibeshell exec <session-id> --command-file <path> should parse"
        );
    }

    #[test]
    fn parses_ssh_session_interactive() {
        let parsed = Cli::try_parse_from(["vibeshell", "ssh-session", "001"]);
        assert!(parsed.is_ok(), "vibeshell ssh-session <alias> should parse");
    }

    #[test]
    fn parses_ssh_session_with_command() {
        let parsed = Cli::try_parse_from(["vibeshell", "ssh-session", "001", "--", "uname", "-a"]);
        assert!(
            parsed.is_ok(),
            "vibeshell ssh-session <alias> -- <cmd> should parse"
        );
    }

    #[test]
    fn parses_ssh_session_command_stdin() {
        let parsed = Cli::try_parse_from(["vibeshell", "ssh-session", "001", "--command-stdin"]);
        assert!(
            parsed.is_ok(),
            "vibeshell ssh-session <alias> --command-stdin should parse"
        );
    }

    #[test]
    fn parses_ssh_session_alias_ss() {
        let parsed = Cli::try_parse_from(["vibeshell", "ss", "001"]);
        assert!(
            parsed.is_ok(),
            "vibeshell ss <alias> should parse (alias for ssh-session)"
        );
    }

    #[test]
    fn parses_send_key_command() {
        let parsed = Cli::try_parse_from(["vibeshell", "send-key", "001", "y", "enter"]);
        assert!(
            parsed.is_ok(),
            "vibeshell send-key <alias> <keys...> should parse"
        );
    }

    #[test]
    fn parses_send_key_alias_sk() {
        let parsed = Cli::try_parse_from(["vibeshell", "sk", "001", "ctrl-c"]);
        assert!(
            parsed.is_ok(),
            "vibeshell sk <alias> <key> should parse (alias for send-key)"
        );
    }
}
