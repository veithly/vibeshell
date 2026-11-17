//! Container identity is immutable and shared by terminal, quick exec, MCP and
//! plugins. Never fall through to host execution or host SFTP.
use anyhow::{bail, ensure, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerContext {
    pub parent_session_id: String,
    pub container_id: String,
    pub name: String,
    pub user: Option<String>,
    pub workdir: Option<String>,
    pub shell: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerOpenRequest {
    pub parent_session_id: String,
    pub container: String,
    pub user: Option<String>,
    pub workdir: Option<String>,
    #[serde(default = "default_shell")]
    pub shell: String,
}
fn default_shell() -> String {
    "/bin/sh".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerInfo {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "Names")]
    pub name: String,
    #[serde(rename = "Image")]
    pub image: String,
    #[serde(rename = "State")]
    pub state: String,
    #[serde(rename = "Status")]
    pub status: String,
}

#[derive(Deserialize)]
pub(crate) struct Inspect {
    pub id: String,
    pub name: String,
    pub running: bool,
}

pub(crate) fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
pub(crate) fn validate_request(request: &ContainerOpenRequest) -> Result<()> {
    ensure!(
        !request.container.is_empty()
            && request.container.len() <= 255
            && !request.container.starts_with('-')
            && request
                .container
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b)),
        "Invalid container ID or name"
    );
    ensure!(
        matches!(
            request.shell.as_str(),
            "/bin/sh" | "/bin/bash" | "/bin/ash" | "/bin/zsh"
        ),
        "Select an explicit supported container shell"
    );
    if let Some(user) = &request.user {
        ensure!(
            !user.is_empty()
                && user.len() <= 128
                && !user.starts_with('-')
                && user
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_.-:".contains(&b)),
            "Invalid container user"
        );
    }
    if let Some(dir) = &request.workdir {
        ensure!(
            dir.starts_with('/') && dir.len() <= 4096 && !dir.chars().any(char::is_control),
            "Container directory must be an absolute path without control characters"
        );
    }
    Ok(())
}

impl ContainerContext {
    pub(crate) fn from_inspect(request: ContainerOpenRequest, inspect: Inspect) -> Result<Self> {
        validate_request(&request)?;
        ensure!(
            inspect.id.len() == 64 && inspect.id.bytes().all(|c| c.is_ascii_hexdigit()),
            "Docker did not return a full immutable container ID"
        );
        if !inspect.running {
            bail!("Container is stopped; VibeShell will not start it automatically");
        }
        Ok(Self {
            parent_session_id: request.parent_session_id,
            container_id: inspect.id,
            name: inspect.name.trim_start_matches('/').into(),
            user: request.user,
            workdir: request.workdir,
            shell: request.shell,
        })
    }

    pub fn command(&self, command: &str, terminal: bool) -> String {
        let mut args = vec![
            "docker exec".to_owned(),
            if terminal { "-it" } else { "-i" }.into(),
        ];
        if let Some(user) = &self.user {
            args.extend(["--user".into(), quote(user)]);
        }
        if let Some(dir) = &self.workdir {
            args.extend(["--workdir".into(), quote(dir)]);
        }
        args.extend([
            "--".into(),
            quote(&self.container_id),
            quote(&self.shell),
            "-c".into(),
            quote(command),
        ]);
        args.join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> ContainerOpenRequest {
        ContainerOpenRequest {
            parent_session_id: "p".into(),
            container: "web".into(),
            user: Some("1000:1000".into()),
            workdir: Some("/app/中文 ' directory".into()),
            shell: default_shell(),
        }
    }
    #[test]
    fn context_uses_full_id_and_safe_arguments_for_every_exec() {
        let c = ContainerContext::from_inspect(
            request(),
            Inspect {
                id: "a".repeat(64),
                name: "/web".into(),
                running: true,
            },
        )
        .unwrap();
        let command = c.command("printf '%s' \"$HOME\"", false);
        assert!(command.contains(&format!("-- '{}'", "a".repeat(64))));
        assert!(command.contains("--user '1000:1000'"));
        assert!(command.contains("'\\''"));
        assert!(!command.contains("sudo"));
        assert!(c.command("exit", true).starts_with("docker exec -it"));
    }
    #[test]
    fn rejects_stopped_selectors_and_shell_injection() {
        for value in ["--privileged", "a;id", "$(id)", "a\nb", ""] {
            let mut r = request();
            r.container = value.into();
            assert!(validate_request(&r).is_err());
        }
        assert!(ContainerContext::from_inspect(
            request(),
            Inspect {
                id: "a".repeat(64),
                name: "web".into(),
                running: false
            }
        )
        .is_err());
    }
}
