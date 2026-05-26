use std::fs::{self, OpenOptions};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use vibeshell_core::ipc::{IpcClient, IpcEndpointStatus, IpcMessage};

const STARTUP_TIMEOUT: Duration = Duration::from_secs(5);

pub fn run_foreground() -> Result<()> {
    run_foreground_with(vibeshell_core::IpcServer::new)
}

trait HeadlessDaemon {
    fn run(&self) -> Result<()>;
}

impl HeadlessDaemon for vibeshell_core::IpcServer {
    fn run(&self) -> Result<()> {
        self.run().map_err(anyhow::Error::new)
    }
}

fn run_foreground_with<F, S>(build_server: F) -> Result<()>
where
    F: FnOnce(Arc<vibeshell_core::Database>, Arc<vibeshell_core::SessionManager>) -> S,
    S: HeadlessDaemon,
{
    let database = Arc::new(vibeshell_core::Database::new().context("Failed to open database")?);
    let session_manager = Arc::new(vibeshell_core::SessionManager::new(database.clone()));

    eprintln!("Starting VibeShell headless daemon...");
    let server = build_server(database, session_manager);
    server.run()
}

fn daemon_log_path() -> std::path::PathBuf {
    std::env::temp_dir().join("vshell-daemon.log")
}

fn daemon_log_len() -> u64 {
    fs::metadata(daemon_log_path())
        .map(|metadata| metadata.len())
        .unwrap_or(0)
}

fn recent_daemon_diagnostic(log_len_before_spawn: u64) -> Option<String> {
    let log_path = daemon_log_path();
    let bytes = fs::read(&log_path).ok()?;
    let start = usize::try_from(log_len_before_spawn)
        .ok()
        .filter(|offset| *offset <= bytes.len())
        .unwrap_or(0);
    let text = String::from_utf8_lossy(&bytes[start..]);

    text.lines()
        .rev()
        .map(str::trim)
        .find(|line| {
            !line.is_empty()
                && (line.starts_with("Error:")
                    || line.contains("panicked at")
                    || line.starts_with("thread 'main'"))
        })
        .map(str::to_string)
}

fn should_start_daemon(status: IpcEndpointStatus) -> Result<bool> {
    match status {
        IpcEndpointStatus::Reachable => Ok(false),
        IpcEndpointStatus::NotRunning => Ok(true),
        IpcEndpointStatus::Occupied => Ok(false),
    }
}

fn is_ipc_reachable() -> bool {
    IpcClient::send(&IpcMessage::ListSessions).is_ok()
}

fn wait_for_reachable(log_len_before_spawn: Option<u64>) -> Result<()> {
    let deadline = Instant::now() + STARTUP_TIMEOUT;
    let mut saw_occupied = false;

    while Instant::now() < deadline {
        if is_ipc_reachable() {
            return Ok(());
        }

        match IpcClient::endpoint_status() {
            IpcEndpointStatus::Reachable => return Ok(()),
            IpcEndpointStatus::Occupied => {
                saw_occupied = true;
            }
            IpcEndpointStatus::NotRunning => {}
        }
        thread::sleep(Duration::from_millis(150));
    }

    if let Some(before) = log_len_before_spawn {
        if let Some(diagnostic) = recent_daemon_diagnostic(before) {
            bail!(
                "Timed out waiting for the VibeShell headless daemon to start. Last daemon log: {}",
                diagnostic
            );
        }
    }

    if saw_occupied {
        bail!(
            "VibeShell IPC endpoint {} stayed occupied but never became reachable.",
            IpcClient::socket_name_display()
        );
    }

    bail!("Timed out waiting for the VibeShell headless daemon to start.")
}

pub fn print_status() {
    match IpcClient::endpoint_status() {
        IpcEndpointStatus::Reachable => println!("VibeShell daemon is running."),
        IpcEndpointStatus::Occupied => {
            println!("VibeShell IPC endpoint is occupied but not accepting CLI connections.")
        }
        IpcEndpointStatus::NotRunning => println!("VibeShell daemon is not running."),
    }
}

pub fn ensure_running() -> Result<()> {
    if is_ipc_reachable() {
        return Ok(());
    }

    match IpcClient::endpoint_status() {
        IpcEndpointStatus::Reachable => return wait_for_reachable(None),
        IpcEndpointStatus::Occupied => {
            if wait_for_reachable(None).is_ok() {
                return Ok(());
            }

            let log_len_before_spawn = daemon_log_len();
            start_background_force()?;
            return wait_for_reachable(Some(log_len_before_spawn));
        }
        IpcEndpointStatus::NotRunning => {}
    }

    let log_len_before_spawn = daemon_log_len();
    start_background()?;
    wait_for_reachable(Some(log_len_before_spawn))
}

pub fn start_background() -> Result<()> {
    if !should_start_daemon(IpcClient::endpoint_status())? {
        return Ok(());
    }

    start_background_force()
}

fn start_background_force() -> Result<()> {
    let current_exe = std::env::current_exe().context("Failed to resolve current executable")?;
    let log_path = daemon_log_path();
    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("Failed to open daemon log file at {}", log_path.display()))?;
    let stderr = stdout
        .try_clone()
        .context("Failed to clone daemon log file handle")?;

    let mut command = Command::new(current_exe);
    command
        .arg("daemon")
        .arg("run")
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        const DETACHED_PROCESS: u32 = 0x00000008;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW);
    }

    command.spawn().context("Failed to spawn headless daemon")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{run_foreground_with, should_start_daemon, HeadlessDaemon};

    use anyhow::{bail, Result};
    use vibeshell_core::ipc::IpcEndpointStatus;

    struct FailingServer {
        _runtime: tokio::runtime::Runtime,
    }

    impl HeadlessDaemon for FailingServer {
        fn run(&self) -> Result<()> {
            bail!("listener already bound")
        }
    }

    #[test]
    fn run_foreground_returns_error_instead_of_panicking_when_server_setup_fails() {
        let result = std::panic::catch_unwind(|| {
            run_foreground_with(|_, _| FailingServer {
                _runtime: tokio::runtime::Runtime::new().expect("create nested runtime"),
            })
        });
        assert!(result.is_ok(), "run_foreground should not panic");

        let error = result
            .expect("run_foreground should return a Result")
            .expect_err("run_foreground should fail when the daemon server returns an error");

        assert!(
            error.to_string().contains("listener already bound"),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn should_start_daemon_returns_false_when_reachable() {
        let should_start = should_start_daemon(IpcEndpointStatus::Reachable)
            .expect("reachable status should not error");
        assert!(!should_start);
    }

    #[test]
    fn should_start_daemon_returns_true_when_not_running() {
        let should_start = should_start_daemon(IpcEndpointStatus::NotRunning)
            .expect("not running status should not error");
        assert!(should_start);
    }

    #[test]
    fn should_start_daemon_fails_when_occupied() {
        let should_start = should_start_daemon(IpcEndpointStatus::Occupied)
            .expect("occupied status should not error");
        assert!(!should_start);
    }
}
