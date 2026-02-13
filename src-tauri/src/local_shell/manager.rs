//! Local shell session manager.

use anyhow::{Result, anyhow};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use log::{info, warn, error};

use super::{LocalShellSession, LocalShellInfo, ShellInfo, detect_available_shells, get_default_shell};

/// Manages local shell sessions
pub struct LocalShellManager {
    sessions: Arc<RwLock<HashMap<String, Arc<LocalShellSession>>>>,
}

impl LocalShellManager {
    /// Create a new local shell manager
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// List all available shells on the system
    pub fn list_shells(&self) -> Vec<ShellInfo> {
        detect_available_shells()
    }

    /// Get the default shell
    pub fn get_default_shell(&self) -> Option<ShellInfo> {
        get_default_shell()
    }

    /// List all active sessions
    pub async fn list_sessions(&self) -> Vec<LocalShellInfo> {
        let sessions = self.sessions.read().await;
        let mut infos = Vec::new();
        for session in sessions.values() {
            infos.push(session.get_info().await);
        }
        infos
    }

    /// Get a session by ID
    pub async fn get_session(&self, id: &str) -> Option<Arc<LocalShellSession>> {
        let sessions = self.sessions.read().await;
        sessions.get(id).cloned()
    }

    /// Create a new local shell session
    pub async fn create_session(
        &self,
        shell_id: &str,
        cols: u16,
        rows: u16,
    ) -> Result<Arc<LocalShellSession>> {
        info!("[LocalShellManager] Creating session for shell: {}", shell_id);

        // Find the shell info
        let shells = detect_available_shells();
        let shell_info = shells.iter()
            .find(|s| s.id == shell_id)
            .ok_or_else(|| {
                error!("[LocalShellManager] Shell not found: {}", shell_id);
                anyhow!("Shell not found: {}", shell_id)
            })?;

        // Create the session
        let session = LocalShellSession::new(shell_info, cols, rows)?;
        let session = Arc::new(session);

        info!("[LocalShellManager] Session created with ID: {}", session.id);

        // Store the session
        let mut sessions = self.sessions.write().await;
        sessions.insert(session.id.clone(), session.clone());

        Ok(session)
    }

    /// Create a session with the default shell
    pub async fn create_default_session(
        &self,
        cols: u16,
        rows: u16,
    ) -> Result<Arc<LocalShellSession>> {
        let default_shell = get_default_shell()
            .ok_or_else(|| anyhow!("No default shell found"))?;

        self.create_session(&default_shell.id, cols, rows).await
    }

    /// Kill a session
    pub async fn kill_session(&self, id: &str) -> Result<()> {
        info!("[LocalShellManager] Killing session: {}", id);

        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.remove(id) {
            session.stop().await?;
            info!("[LocalShellManager] Session {} killed successfully", id);
        } else {
            warn!("[LocalShellManager] Session {} not found for kill", id);
        }

        Ok(())
    }

    /// Kill all sessions
    pub async fn kill_all(&self) -> Result<()> {
        info!("[LocalShellManager] Killing all sessions");

        let mut sessions = self.sessions.write().await;
        let count = sessions.len();

        for (id, session) in sessions.iter() {
            if let Err(e) = session.stop().await {
                error!("[LocalShellManager] Error stopping session {}: {}", id, e);
            }
        }

        sessions.clear();
        info!("[LocalShellManager] Killed {} sessions", count);

        Ok(())
    }
}

impl Default for LocalShellManager {
    fn default() -> Self {
        Self::new()
    }
}
