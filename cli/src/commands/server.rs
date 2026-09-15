//! Server management commands for the CLI.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use vibeshell_core::commands::server::AddServerSpec;
use vibeshell_core::ipc::IpcMessage;

use crate::ipc_support;
use crate::ssh_target;

pub struct AddServerArgs {
    pub target: String,
    pub name: Option<String>,
    pub user: Option<String>,
    pub port: Option<u16>,
    pub identity: Option<PathBuf>,
    pub jump: Option<String>,
    pub agent_forwarding: bool,
    pub post_login: Option<String>,
    pub group: Option<String>,
    pub tags: Vec<String>,
    pub connection_kind: String,
    pub teleport_proxy: Option<String>,
}

/// List all configured servers known to VibeShell.
pub fn list() -> Result<()> {
    let response = ipc_support::send(&IpcMessage::ListServers)?;

    match response {
        IpcMessage::ServerList { servers } => {
            if servers.is_empty() {
                println!("No configured servers.");
                return Ok(());
            }

            println!("Configured servers:");
            for server in servers {
                let kind = server.connection_kind.as_deref().unwrap_or("ssh");
                if kind == "teleport" {
                    println!(
                        "  {}  {}@{}  teleport proxy={}",
                        server.name,
                        server.username,
                        server.host,
                        server.teleport_proxy.as_deref().unwrap_or("-")
                    );
                } else {
                    println!(
                        "  {}  {}@{}:{}  auth={}",
                        server.name, server.username, server.host, server.port, server.auth_type
                    );
                }
            }
            Ok(())
        }
        IpcMessage::Error { message } => {
            bail!("Error listing servers: {}", message);
        }
        _ => {
            bail!("Unexpected response from background service");
        }
    }
}

/// Add a server from `user@host[:port]` shorthand. Secrets come from env vars,
/// never from argv: `SSH_PASSWORD` or `VIBESHELL_PASSWORD`, and
/// `VIBESHELL_KEY_PASSPHRASE` when `--identity` points at an encrypted key.
pub fn add(args: AddServerArgs) -> Result<()> {
    let target = ssh_target::parse_ssh_target(&args.target)?;
    let username = args
        .user
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or(target.username)
        .ok_or_else(|| anyhow::anyhow!("Username is required (use user@host or pass --user)"))?;
    let host = target.host;
    let port = args.port.or(target.port).unwrap_or(22);
    let name = args
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| host.clone());

    let identity_path = args.identity.as_deref();
    let is_teleport = args.connection_kind.eq_ignore_ascii_case("teleport")
        || args.connection_kind.eq_ignore_ascii_case("tsh");
    if is_teleport {
        let proxy_ok = args
            .teleport_proxy
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some();
        if !proxy_ok {
            bail!("Teleport servers require --proxy (for example teleport.example.com:443)");
        }
    }

    let (auth_type, credential, passphrase, key_path, saved_credentials) = if is_teleport {
        ("password", None, None, None, true)
    } else {
        resolve_credentials(identity_path)?
    };

    let spec = AddServerSpec {
        name: name.clone(),
        host: host.clone(),
        port,
        username: username.clone(),
        auth_type: auth_type.to_string(),
        group_id: None,
        group_name: args.group,
        tags: args.tags,
        jump_host_id: None,
        jump_host: args.jump,
        post_login_command: args.post_login,
        agent_forwarding: args.agent_forwarding,
        connection_kind: Some(args.connection_kind),
        teleport_proxy: args.teleport_proxy,
        credential,
        passphrase,
        key_path,
    };

    match ipc_support::send(&IpcMessage::AddServer { spec })? {
        IpcMessage::ServerAdded { server } => {
            println!(
                "Added server '{}' ({}@{}:{})",
                server.name, server.username, server.host, server.port
            );
            if !saved_credentials {
                if identity_path.is_some() {
                    eprintln!(
                        "Warning: --identity was set but the key file could not be stored. This should not happen."
                    );
                } else {
                    eprintln!(
                        "Credentials were not saved. Set SSH_PASSWORD or VIBESHELL_PASSWORD to store a password, or pass --identity KEYFILE."
                    );
                }
            }
            Ok(())
        }
        IpcMessage::Error { message } => {
            bail!("Error adding server: {}", message);
        }
        _ => bail!("Unexpected response from background service"),
    }
}

pub fn delete(name: &str) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        bail!("Server name is required");
    }

    match ipc_support::send(&IpcMessage::DeleteServer {
        name: name.to_string(),
    })? {
        IpcMessage::Ok => {
            println!("Deleted server '{name}'");
            Ok(())
        }
        IpcMessage::Error { message } => {
            bail!("Error deleting server: {}", message);
        }
        _ => bail!("Unexpected response from background service"),
    }
}

fn resolve_credentials(
    identity: Option<&Path>,
) -> Result<(
    &'static str,
    Option<String>,
    Option<String>,
    Option<String>,
    bool,
)> {
    if let Some(path) = identity {
        let key = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read identity file {}", path.display()))?;
        if key.trim().is_empty() {
            bail!("Identity file {} is empty", path.display());
        }
        let passphrase =
            env_nonempty("VIBESHELL_KEY_PASSPHRASE").or_else(|| env_nonempty("SSH_KEY_PASSPHRASE"));
        Ok((
            "key_with_passphrase",
            Some(key),
            passphrase,
            Some(path.display().to_string()),
            true,
        ))
    } else if let Some(password) =
        env_nonempty("SSH_PASSWORD").or_else(|| env_nonempty("VIBESHELL_PASSWORD"))
    {
        Ok(("password", Some(password), None, None, true))
    } else {
        Ok(("password", None, None, None, false))
    }
}

fn env_nonempty(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::env_nonempty;

    #[test]
    fn env_nonempty_treats_blank_as_unset() {
        std::env::set_var("VIBESHELL_TEST_EMPTY_SECRET", "   ");
        assert!(env_nonempty("VIBESHELL_TEST_EMPTY_SECRET").is_none());
        std::env::remove_var("VIBESHELL_TEST_EMPTY_SECRET");
    }
}
