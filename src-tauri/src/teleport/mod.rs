//! Teleport transport through the user's installed `tsh` and existing login.
//!
//! Never reads Teleport keys, disables verification, requests access or logs in
//! automatically. The shared Session remains the owner for GUI, CLI and MCP.
mod process;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod pty;
#[cfg(all(test, unix, not(any(target_os = "android", target_os = "ios"))))]
mod transport_tests;

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use std::time::Duration;
use tokio::sync::watch;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub use pty::{spawn_tsh_ssh, TeleportPty};

#[derive(Debug, Clone)]
pub struct TeleportTarget {
    pub proxy: String,
    pub login: String,
    pub node: String,
    binary: String,
    cancellation: watch::Sender<bool>,
}

impl TeleportTarget {
    pub fn from_server(server: &crate::storage::Server) -> Result<Self> {
        Self::new(
            server.teleport_proxy.as_deref().unwrap_or_default(),
            &server.username,
            &server.host,
        )
    }

    pub fn new(proxy: &str, login: &str, node: &str) -> Result<Self> {
        validate_proxy(proxy)?;
        validate_operand(login, "Unix login")?;
        validate_operand(node, "node")?;
        Ok(Self {
            proxy: proxy.into(),
            login: login.into(),
            node: node.into(),
            binary: "tsh".into(),
            cancellation: watch::channel(false).0,
        })
    }

    pub fn cancel(&self) {
        self.cancellation.send_replace(true);
    }

    pub(crate) fn cancellation(&self) -> watch::Receiver<bool> {
        self.cancellation.subscribe()
    }

    pub(crate) fn ssh_args(&self, tty: bool, command: &str) -> Vec<String> {
        vec![
            "--proxy".into(),
            self.proxy.clone(),
            "ssh".into(),
            "--no-relogin".into(),
            "--request-mode=off".into(),
            if tty { "--tty" } else { "--no-tty" }.into(),
            "--".into(),
            format!("{}@{}", self.login, self.node),
            command.into(),
        ]
    }

    pub async fn exec(
        &self,
        command: &str,
        stdin: Option<&str>,
        timeout: Duration,
        cap: usize,
    ) -> Result<crate::ssh::client::CommandResult> {
        crate::network::require_native_route_compatible_transport("Teleport tsh")?;
        let output = process::run(
            &self.binary,
            &self.ssh_args(false, command),
            stdin.map(str::as_bytes),
            timeout,
            cap,
            self.cancellation(),
        )
        .await?;
        // Text exec is distinct from binary SFTP. Invalid text fails rather
        // than silently changing bytes or claiming success.
        let mut text =
            String::from_utf8(output.stdout).context("Teleport command output is not UTF-8")?;
        text.push_str(
            &String::from_utf8(output.stderr)
                .context("Teleport command error output is not UTF-8")?,
        );
        Ok(crate::ssh::client::CommandResult {
            output: text,
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    pub async fn open_sftp(&self) -> Result<russh_sftp::client::SftpSession> {
        crate::network::require_native_route_compatible_transport("Teleport tsh")?;
        // Execute a real protocol server; never approximate SFTP with cat/ls/scp.
        // No path supplied by a file request is interpolated into this command.
        const SERVER: &str = "if command -v sftp-server >/dev/null 2>&1; then exec sftp-server; fi; for p in /usr/lib/openssh/sftp-server /usr/libexec/openssh/sftp-server /usr/lib/ssh/sftp-server /usr/libexec/sftp-server; do if test -x \"$p\"; then exec \"$p\"; fi; done; printf '%s\\n' 'VibeShell: a standard sftp-server executable is required on this Teleport node' >&2; exit 127";
        let stream = process::stream(
            &self.binary,
            &self.ssh_args(false, SERVER),
            self.cancellation(),
        )
        .await?;
        let sftp = tokio::time::timeout(Duration::from_secs(20), russh_sftp::client::SftpSession::new_opts(stream, Some(15))).await
            .map_err(|_| anyhow!("Teleport SFTP negotiation timed out; no file operation was performed"))?
            .map_err(|_| anyhow!("Teleport SFTP unavailable: this node must allow execution of a standard OpenSSH sftp-server. No cat/ls/scp fallback is used"))?;
        Ok(sftp)
    }
}

pub fn validate_proxy(proxy: &str) -> Result<()> {
    if proxy.is_empty()
        || proxy.starts_with('-')
        || proxy.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        bail!("A Teleport proxy host[:port] is required");
    }
    let parsed = reqwest::Url::parse(&format!("https://{proxy}"))?;
    if parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.path() != "/"
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || proxy.contains(['/', '@', '?', '#'])
    {
        bail!("Teleport proxy must be a host[:port], not a URL, path or credential");
    }
    if parsed.port() == Some(0) {
        bail!("Teleport proxy port must be nonzero");
    }
    Ok(())
}

fn validate_operand(value: &str, label: &str) -> Result<()> {
    // Label selectors/wildcards can execute on multiple machines; a saved
    // session must refer to one concrete node and one explicit Unix login.
    if value.is_empty()
        || value.starts_with('-')
        || value
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || "@=,*?/'\"\\;[]".contains(c))
    {
        bail!("Teleport {label} must be a single explicit name, not a selector or option");
    }
    Ok(())
}

#[derive(Deserialize)]
struct TeleportNode {
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    hostname: Option<String>,
    #[serde(default)]
    metadata: Option<NodeMetadata>,
    #[serde(default)]
    spec: Option<NodeSpec>,
}
#[derive(Deserialize)]
struct NodeMetadata {
    name: String,
}
#[derive(Deserialize)]
struct NodeSpec {
    #[serde(default)]
    hostname: Option<String>,
}

pub fn parse_tsh_ls_json(json: &str) -> Result<Vec<String>> {
    let nodes: Vec<TeleportNode> = serde_json::from_str(json).context("Invalid tsh ls JSON")?;
    let mut result = Vec::new();
    for node in nodes {
        if node.kind.as_deref().is_some_and(|k| k != "node") {
            continue;
        }
        // Resource UUID/name is unambiguous; hostnames can be shared by nodes.
        let name = node
            .metadata
            .map(|m| m.name)
            .or(node.hostname)
            .or_else(|| node.spec.and_then(|s| s.hostname))
            .ok_or_else(|| anyhow!("Teleport node has no usable identity"))?;
        validate_operand(&name, "node")?;
        if !result.contains(&name) {
            result.push(name);
        }
    }
    Ok(result)
}

/// Import requires an explicit proxy and Unix login. A Teleport identity such
/// as an email address is not an operating-system login and must not be guessed.
pub fn preview_nodes(proxy: &str, login: &str) -> Result<Vec<String>> {
    crate::network::require_native_route_compatible_transport("Teleport tsh")?;
    let target = TeleportTarget::new(proxy, login, "preview")?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let args = vec![
        "--proxy".into(),
        proxy.into(),
        "ls".into(),
        "--format=json".into(),
    ];
    let output = runtime.block_on(process::run(
        "tsh",
        &args,
        None,
        Duration::from_secs(20),
        4 * 1024 * 1024,
        target.cancellation(),
    ))?;
    if !output.status.success() {
        bail!(
            "tsh ls failed; run tsh login for this proxy explicitly before importing (exit {:?})",
            output.status.code()
        );
    }
    parse_tsh_ls_json(
        std::str::from_utf8(&output.stdout).context("tsh ls returned non-UTF-8 JSON")?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_selectors_options_credentials_and_preserves_case() {
        assert_eq!(
            TeleportTarget::new("example.com:443", "CaseSensitive", "node-1")
                .unwrap()
                .login,
            "CaseSensitive"
        );
        for node in ["env=prod", "*", "a,b", "-o", "x@y", "a\nb"] {
            assert!(TeleportTarget::new("example.com", "u", node).is_err());
        }
        for proxy in [
            "",
            "--insecure",
            "user:pass@host",
            "https://host",
            "host/path",
            "host?x",
            "host:0",
            "host:99999",
        ] {
            assert!(validate_proxy(proxy).is_err(), "accepted proxy {proxy}");
        }
        assert!(validate_proxy("[::1]:443").is_ok());
    }
    #[test]
    fn import_uses_unique_node_identity_and_refuses_malformed_data() {
        assert_eq!(parse_tsh_ls_json(r#"[{"kind":"node","metadata":{"name":"UUID-1"},"spec":{"hostname":"shared"}},{"kind":"app","metadata":{"name":"ignore"}}]"#).unwrap(), ["UUID-1"]);
        assert!(parse_tsh_ls_json("not JSON").is_err());
        assert!(parse_tsh_ls_json(r#"[{"kind":"node"}]"#).is_err());
        let args = TeleportTarget::new("proxy", "u", "n")
            .unwrap()
            .ssh_args(false, "echo hello");
        assert!(args.contains(&"--no-relogin".into()));
        assert!(args.contains(&"--request-mode=off".into()));
        assert!(!args.contains(&"--insecure".into()));
    }
}
