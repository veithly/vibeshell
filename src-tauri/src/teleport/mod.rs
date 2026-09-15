//! Teleport (`tsh`) integration.
//!
//! Interactive sessions spawn `tsh ssh` in a local PTY. File and exec
//! operations call `tsh scp` / `tsh ssh -- <command>` so russh is not used.

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::Arc;

use anyhow::{anyhow, bail, Context, Result};
use log::{info, warn};
use serde::Deserialize;

use crate::commands::sftp::SftpEntry;
use crate::storage::Server;

#[derive(Debug, Clone)]
pub struct TeleportTarget {
    pub proxy: String,
    pub login: String,
    pub node: String,
}

impl TeleportTarget {
    pub fn from_server(server: &Server) -> Result<Self> {
        let proxy = server
            .teleport_proxy
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow!("Teleport server '{}' is missing a proxy", server.name))?;
        Ok(Self {
            proxy: proxy.to_string(),
            login: server.username.clone(),
            node: server.host.clone(),
        })
    }

    pub fn ssh_destination(&self) -> String {
        if self.login.is_empty() {
            self.node.clone()
        } else {
            format!("{}@{}", self.login, self.node)
        }
    }
}

pub fn tsh_binary() -> Result<String> {
    Ok("tsh".to_string())
}

pub fn ensure_logged_in(proxy: &str) -> Result<()> {
    let output = tsh_output(["status", "--proxy", proxy])?;
    let combined = format!("{}\n{}", output.stdout, output.stderr);
    if output.status.success() && logged_in_from_status(&combined) {
        return Ok(());
    }
    bail!(
        "tsh is not logged in to proxy '{proxy}'. Run: tsh login --proxy={proxy}\n{}",
        combined.trim()
    );
}

pub fn logged_in_from_status(status: &str) -> bool {
    let lower = status.to_ascii_lowercase();
    if lower.contains("not logged in") || lower.contains("no active profile") {
        return false;
    }
    status.lines().any(|line| {
        let trimmed = line.trim();
        trimmed.to_ascii_lowercase().starts_with("logged in as")
            || trimmed.to_ascii_lowercase().contains("logged in as:")
    })
}

pub fn parse_proxy_from_status(status: &str) -> Option<String> {
    for line in status.lines() {
        let trimmed = line.trim().trim_start_matches('>').trim();
        let Some((label, value)) = trimmed.split_once(':') else {
            continue;
        };
        let label = label.trim().to_ascii_lowercase();
        if label == "profile url" || label == "proxy" {
            let host = strip_url_scheme(value.trim());
            if !host.is_empty() {
                return Some(host.trim_end_matches('/').to_string());
            }
        }
    }
    None
}

fn strip_url_scheme(value: &str) -> String {
    value
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .to_string()
}

pub struct CommandOutput {
    pub status: std::process::ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

pub fn tsh_output<const N: usize>(args: [&str; N]) -> Result<CommandOutput> {
    tsh_output_slice(&args)
}

pub fn tsh_output_slice(args: &[&str]) -> Result<CommandOutput> {
    let binary = tsh_binary()?;
    let output = Command::new(&binary)
        .args(args)
        .output()
        .with_context(|| format!("Failed to run `{binary} {}`", args.join(" ")))?;
    Ok(CommandOutput {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

pub fn tsh_ssh_exec(target: &TeleportTarget, command: &str) -> Result<String> {
    tsh_ssh_exec_with_stdin(target, command, None)
}

pub fn tsh_ssh_exec_with_stdin(
    target: &TeleportTarget,
    command: &str,
    stdin: Option<&str>,
) -> Result<String> {
    ensure_logged_in(&target.proxy)?;
    let dest = target.ssh_destination();
    let mut child = Command::new(tsh_binary()?)
        .args(["--proxy", &target.proxy, "ssh", &dest, "--", command])
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("Failed to start tsh ssh {dest}"))?;
    if let Some(payload) = stdin {
        let mut handle = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("tsh stdin closed"))?;
        handle.write_all(payload.as_bytes())?;
    }
    let output = child.wait_with_output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if !output.status.success() {
        let detail = stderr.trim();
        bail!(
            "tsh ssh failed on {} (proxy {}): {}",
            dest,
            target.proxy,
            if detail.is_empty() {
                stdout.trim()
            } else {
                detail
            }
        );
    }
    Ok(stdout)
}

pub fn teleport_home(target: &TeleportTarget) -> Result<String> {
    let pwd = tsh_ssh_exec(target, "pwd")?;
    let home = pwd.lines().last().unwrap_or(pwd.trim()).trim();
    if home.is_empty() {
        bail!("Could not determine home directory via tsh ssh");
    }
    Ok(home.to_string())
}

pub fn list_dir(target: &TeleportTarget, path: &str) -> Result<Vec<SftpEntry>> {
    match tsh_ssh_exec(target, &python_stat_command("listdir", path)) {
        Ok(stdout) => {
            if let Ok(entries) = serde_json::from_str::<Vec<SftpEntry>>(stdout.trim()) {
                return Ok(entries);
            }
        }
        Err(error) => warn!("[Teleport] python listdir failed: {error}"),
    }

    let listing = tsh_ssh_exec(
        target,
        &format!("ls -1A {}", shell_single_quote(path)),
    )?;
    Ok(listing
        .lines()
        .map(|name| name.trim())
        .filter(|name| !name.is_empty())
        .map(|name| {
            let entry_path = if path.ends_with('/') {
                format!("{path}{name}")
            } else {
                format!("{path}/{name}")
            };
            SftpEntry {
                name: name.to_string(),
                path: entry_path,
                is_directory: false,
                size: 0,
                modified_at: 0,
                permissions: "-".to_string(),
            }
        })
        .collect())
}

pub fn stat_path(target: &TeleportTarget, path: &str) -> Result<SftpEntry> {
    let stdout = tsh_ssh_exec(target, &python_stat_command("stat", path))?;
    serde_json::from_str(stdout.trim()).context("Failed to parse Teleport stat JSON")
}

pub fn read_file(target: &TeleportTarget, path: &str, max_size: Option<u64>) -> Result<String> {
    let escaped = shell_single_quote(path);
    let command = match max_size {
        Some(limit) => format!("head -c {limit} -- {escaped}"),
        None => format!("cat -- {escaped}"),
    };
    tsh_ssh_exec(target, &command)
}

pub fn write_file(target: &TeleportTarget, path: &str, content: &str) -> Result<()> {
    tsh_ssh_exec_with_stdin(
        target,
        &format!("cat > {}", shell_single_quote(path)),
        Some(content),
    )
    .map(|_| ())
}

pub fn mkdir(target: &TeleportTarget, path: &str) -> Result<()> {
    tsh_ssh_exec(target, &format!("mkdir -p {}", shell_single_quote(path))).map(|_| ())
}

pub fn delete_path(target: &TeleportTarget, path: &str, recursive: bool) -> Result<()> {
    let flag = if recursive { "-rf" } else { "-f" };
    tsh_ssh_exec(
        target,
        &format!("rm {flag} -- {}", shell_single_quote(path)),
    )
    .map(|_| ())
}

pub fn rename(target: &TeleportTarget, old_path: &str, new_path: &str) -> Result<()> {
    tsh_ssh_exec(
        target,
        &format!(
            "mv -- {} {}",
            shell_single_quote(old_path),
            shell_single_quote(new_path)
        ),
    )
    .map(|_| ())
}

pub fn download_file(target: &TeleportTarget, remote_path: &str, local_path: &str) -> Result<()> {
    ensure_logged_in(&target.proxy)?;
    let source = format!("{}:{}", target.ssh_destination(), remote_path);
    let output = tsh_output_slice(&["--proxy", &target.proxy, "scp", &source, local_path])?;
    if !output.status.success() {
        bail!("tsh scp download failed: {}", output.stderr.trim());
    }
    Ok(())
}

pub fn upload_file(target: &TeleportTarget, local_path: &str, remote_path: &str) -> Result<()> {
    ensure_logged_in(&target.proxy)?;
    let dest = format!("{}:{}", target.ssh_destination(), remote_path);
    let output = tsh_output_slice(&["--proxy", &target.proxy, "scp", local_path, &dest])?;
    if !output.status.success() {
        bail!("tsh scp upload failed: {}", output.stderr.trim());
    }
    Ok(())
}

pub fn upload_directory(
    target: &TeleportTarget,
    local_path: &str,
    remote_path: &str,
) -> Result<()> {
    ensure_logged_in(&target.proxy)?;
    let dest = format!("{}:{}", target.ssh_destination(), remote_path);
    let output = tsh_output_slice(&["--proxy", &target.proxy, "scp", "-r", local_path, &dest])?;
    if !output.status.success() {
        bail!("tsh scp directory upload failed: {}", output.stderr.trim());
    }
    Ok(())
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn python_stat_command(mode: &str, path: &str) -> String {
    let script = r#"import json,os,stat,sys
path=sys.argv[2]
mode=sys.argv[1]
def entry(p):
    st=os.lstat(p)
    return {'name':os.path.basename(p) or p,'path':p,'isDirectory':stat.S_ISDIR(st.st_mode),'size':st.st_size,'modifiedAt':int(st.st_mtime),'permissions':oct(st.st_mode & 0o777)}
if mode=='stat':
    print(json.dumps(entry(path)))
else:
    entries=[]
    for name in os.listdir(path):
        p=os.path.join(path,name)
        try:
            entries.append(entry(p))
        except OSError:
            continue
    print(json.dumps(entries))"#;
    format!(
        "python3 -c {} {} {}",
        shell_single_quote(script),
        shell_single_quote(mode),
        shell_single_quote(path)
    )
}

#[derive(Debug, Deserialize)]
struct TeleportNode {
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    hostname: Option<String>,
    #[serde(default)]
    metadata: Option<TeleportMetadata>,
    #[serde(default)]
    spec: Option<TeleportSpec>,
}

#[derive(Debug, Deserialize)]
struct TeleportMetadata {
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TeleportSpec {
    #[serde(default)]
    hostname: Option<String>,
}

impl TeleportNode {
    fn is_ssh_node(&self) -> bool {
        match self.kind.as_deref() {
            None => true,
            Some(kind) => kind.eq_ignore_ascii_case("node"),
        }
    }

    fn hostname(&self) -> Option<String> {
        self.hostname
            .clone()
            .or_else(|| self.spec.as_ref().and_then(|spec| spec.hostname.clone()))
            .or_else(|| self.metadata.as_ref().and_then(|meta| meta.name.clone()))
            .or_else(|| self.name.clone())
            .filter(|value| !value.is_empty())
    }
}

pub struct TeleportImportPreview {
    pub proxy: String,
    pub login: Option<String>,
    pub nodes: Vec<String>,
    pub warnings: Vec<String>,
}

pub fn preview_import(explicit_proxy: Option<&str>) -> Result<TeleportImportPreview> {
    let status_args = match explicit_proxy {
        Some(proxy) => vec!["status", "--proxy", proxy],
        None => vec!["status"],
    };
    let status = tsh_output_slice(&status_args)?;
    let combined = format!("{}\n{}", status.stdout, status.stderr);
    if !logged_in_from_status(&combined) {
        bail!(
            "tsh is not logged in. Run `tsh login --proxy=<proxy>` first.\n{}",
            combined.trim()
        );
    }
    let proxy = explicit_proxy
        .map(ToOwned::to_owned)
        .or_else(|| parse_proxy_from_status(&combined))
        .ok_or_else(|| anyhow!("Could not determine Teleport proxy from `tsh status`"))?;

    let login = combined.lines().find_map(|line| {
        let lower = line.to_ascii_lowercase();
        lower
            .split_once("logged in as:")
            .map(|(_, rest)| rest.trim().to_string())
            .filter(|value| !value.is_empty())
    });

    let ls = tsh_output_slice(&["--proxy", &proxy, "ls", "--format=json"])?;
    let mut warnings = Vec::new();
    if !ls.status.success() {
        warnings.push(format!("tsh ls failed: {}", ls.stderr.trim()));
    }
    let nodes = parse_tsh_ls_json(&ls.stdout).unwrap_or_else(|error| {
        warnings.push(format!("Could not parse tsh ls JSON: {error}"));
        Vec::new()
    });

    Ok(TeleportImportPreview {
        proxy,
        login,
        nodes,
        warnings,
    })
}

pub fn parse_tsh_ls_json(json: &str) -> Result<Vec<String>> {
    let trimmed = json.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    if let Ok(nodes) = serde_json::from_str::<Vec<TeleportNode>>(trimmed) {
        return Ok(nodes
            .into_iter()
            .filter(TeleportNode::is_ssh_node)
            .filter_map(|node| node.hostname())
            .collect());
    }
    if let Ok(node) = serde_json::from_str::<TeleportNode>(trimmed) {
        return Ok(node.hostname().into_iter().collect());
    }
    bail!("Unrecognized tsh ls JSON");
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub struct TeleportPty {
    pub writer: Arc<std::sync::Mutex<Option<Box<dyn Write + Send>>>>,
    pub master: Arc<std::sync::Mutex<Option<Box<dyn portable_pty::MasterPty + Send>>>>,
    child: Arc<std::sync::Mutex<Option<Box<dyn portable_pty::Child + Send + Sync>>>>,
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn spawn_tsh_ssh(
    target: &TeleportTarget,
    cols: u16,
    rows: u16,
    output_tx: tokio::sync::mpsc::Sender<Vec<u8>>,
) -> Result<TeleportPty> {
    ensure_logged_in(&target.proxy)?;
    let binary = tsh_binary()?;
    let pty_system = portable_pty::native_pty_system();
    let pair = pty_system.openpty(portable_pty::PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    })?;

    let mut cmd = portable_pty::CommandBuilder::new(&binary);
    cmd.arg("--proxy");
    cmd.arg(&target.proxy);
    cmd.arg("ssh");
    cmd.arg(target.ssh_destination());
    #[cfg(not(target_os = "windows"))]
    {
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("TERM_PROGRAM", "VibeShell");
    }

    let child = pair.slave.spawn_command(cmd)?;
    info!(
        "[Teleport] spawned tsh ssh {} via proxy {}",
        target.ssh_destination(),
        target.proxy
    );

    let writer = pair.master.take_writer()?;
    let mut reader = pair.master.try_clone_reader()?;
    let writer = Arc::new(std::sync::Mutex::new(Some(writer)));
    let master = Arc::new(std::sync::Mutex::new(Some(pair.master)));
    let child = Arc::new(std::sync::Mutex::new(Some(child)));

    let child_for_wait = child.clone();
    std::thread::spawn(move || {
        let mut buf = vec![0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if output_tx.blocking_send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        if let Ok(mut guard) = child_for_wait.lock() {
            if let Some(mut child) = guard.take() {
                let _ = child.wait();
            }
        }
    });

    Ok(TeleportPty {
        writer,
        master,
        child,
    })
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
impl TeleportPty {
    pub fn write(&self, data: &[u8]) -> Result<()> {
        let mut guard = self
            .writer
            .lock()
            .map_err(|_| anyhow!("Teleport PTY writer lock poisoned"))?;
        let writer = guard
            .as_mut()
            .ok_or_else(|| anyhow!("Teleport PTY is closed"))?;
        writer.write_all(data)?;
        writer.flush()?;
        Ok(())
    }

    pub fn resize(&self, cols: u32, rows: u32) -> Result<()> {
        let guard = self
            .master
            .lock()
            .map_err(|_| anyhow!("Teleport PTY master lock poisoned"))?;
        let master = guard
            .as_ref()
            .ok_or_else(|| anyhow!("Teleport PTY is closed"))?;
        master.resize(portable_pty::PtySize {
            rows: rows as u16,
            cols: cols as u16,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        Ok(())
    }

    pub fn kill(&self) {
        if let Ok(mut guard) = self.child.lock() {
            if let Some(mut child) = guard.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
        if let Ok(mut writer) = self.writer.lock() {
            *writer = None;
        }
        if let Ok(mut master) = self.master.lock() {
            *master = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{logged_in_from_status, parse_proxy_from_status, parse_tsh_ls_json};

    #[test]
    fn detects_logged_in_status() {
        let status = ">\n  Profile URL:        https://teleport.example.com:443\n  Logged in as:       alice\n  Cluster:            example\n";
        assert!(logged_in_from_status(status));
        assert_eq!(
            parse_proxy_from_status(status).as_deref(),
            Some("teleport.example.com:443")
        );
    }

    #[test]
    fn detects_logged_out_status() {
        assert!(!logged_in_from_status("ERROR: not logged in"));
        assert!(!logged_in_from_status("No active profile"));
    }

    #[test]
    fn parses_node_list_json() {
        let json = r#"[{"kind":"node","metadata":{"name":"web-1"},"spec":{"hostname":"web-1"}}]"#;
        assert_eq!(parse_tsh_ls_json(json).unwrap(), vec!["web-1".to_string()]);
    }
}
