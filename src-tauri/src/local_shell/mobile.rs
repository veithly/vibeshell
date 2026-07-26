use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellInfo {
    pub id: String,
    pub name: String,
    pub path: String,
    pub shell_type: ShellType,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ShellType {
    PowerShell,
    Cmd,
    Bash,
    Zsh,
    Fish,
    Sh,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LocalShellState {
    Starting,
    Running,
    Stopped,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalShellInfo {
    pub id: String,
    pub shell_id: String,
    pub shell_name: String,
    pub cwd: Option<String>,
    pub agent_id: Option<String>,
    pub state: LocalShellState,
    pub created_at: i64,
    pub clients: usize,
}

/// Marker type retained for the cross-platform manager API.
pub struct LocalShellSession;

/// Mobile adapter used by SFTP/session routing. Mobile never has local PTY sessions.
pub struct LocalShellManager;

impl LocalShellManager {
    pub fn new() -> Self {
        Self
    }

    pub async fn get_session(&self, _id: &str) -> Option<LocalShellSession> {
        None
    }
}

impl Default for LocalShellManager {
    fn default() -> Self {
        Self::new()
    }
}
