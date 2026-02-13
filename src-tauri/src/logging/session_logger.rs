use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;
use directories::ProjectDirs;
use log::{info, warn, error};
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::session::Session;
use crate::storage::models::{Recording, SyncStatus};
use crate::storage::Database;

struct LoggerHandle {
    recording_id: String,
    session_id: String,
    abort_handle: tokio::task::AbortHandle,
}

pub struct SessionLogger {
    active_loggers: Arc<RwLock<HashMap<String, LoggerHandle>>>,
    log_dir: PathBuf,
    database: Arc<Database>,
}

impl SessionLogger {
    pub fn new(database: Arc<Database>) -> Self {
        let log_dir = ProjectDirs::from("com", "vibeshell", "VibeShell")
            .map(|dirs| dirs.data_dir().join("recordings"))
            .unwrap_or_else(|| PathBuf::from("recordings"));

        info!("[SessionLogger] Log directory: {:?}", log_dir);

        Self {
            active_loggers: Arc::new(RwLock::new(HashMap::new())),
            log_dir,
            database,
        }
    }

    /// Start recording a session's terminal output
    pub async fn start_recording(
        &self,
        session: Arc<Session>,
        server_id: &str,
    ) -> Result<String> {
        let session_id = session.id.clone();

        // Check if already recording
        {
            let loggers = self.active_loggers.read().await;
            if loggers.values().any(|h| h.session_id == session_id) {
                return Err(anyhow::anyhow!("Session {} is already being recorded", session_id));
            }
        }

        // Create directory for this session's recordings
        let session_dir = self.log_dir.join(&session_id);
        tokio::fs::create_dir_all(&session_dir).await?;

        // Generate file path
        let now = Utc::now();
        let filename = now.format("%Y-%m-%d_%H-%M-%S.log").to_string();
        let file_path = session_dir.join(&filename);
        let file_path_str = file_path.to_string_lossy().to_string();

        // Create recording in database
        let recording_id = Uuid::new_v4().to_string();
        let mut recording = Recording {
            id: recording_id.clone(),
            session_id: session_id.clone(),
            server_id: server_id.to_string(),
            started_at: now.timestamp(),
            ended_at: None,
            file_path: file_path_str.clone(),
            sync_status: SyncStatus::Local,
        };

        self.database.recording_add(&mut recording)?;

        // Subscribe to session output
        let mut receiver = session.subscribe();

        // Spawn the logging task
        let rec_id = recording_id.clone();
        let fp = file_path.clone();
        let task = tokio::spawn(async move {
            let file = match tokio::fs::File::create(&fp).await {
                Ok(f) => f,
                Err(e) => {
                    error!("[SessionLogger] Failed to create log file {:?}: {}", fp, e);
                    return;
                }
            };

            let mut writer = tokio::io::BufWriter::new(file);
            let mut flush_interval = tokio::time::interval(std::time::Duration::from_secs(5));

            info!("[SessionLogger] Started recording {} to {:?}", rec_id, fp);

            loop {
                tokio::select! {
                    result = receiver.recv() => {
                        match result {
                            Ok(data) => {
                                if let Err(e) = writer.write_all(&data).await {
                                    error!("[SessionLogger] Write error for {}: {}", rec_id, e);
                                    break;
                                }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                info!("[SessionLogger] Channel closed for {}", rec_id);
                                break;
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                warn!("[SessionLogger] Lagged {} messages for {}", n, rec_id);
                            }
                        }
                    }
                    _ = flush_interval.tick() => {
                        if let Err(e) = writer.flush().await {
                            error!("[SessionLogger] Flush error for {}: {}", rec_id, e);
                        }
                    }
                }
            }

            // Final flush
            let _ = writer.flush().await;
            info!("[SessionLogger] Finished recording {}", rec_id);
        });

        let abort_handle = task.abort_handle();

        // Store the handle
        let mut loggers = self.active_loggers.write().await;
        loggers.insert(recording_id.clone(), LoggerHandle {
            recording_id: recording_id.clone(),
            session_id,
            abort_handle,
        });

        info!("[SessionLogger] Recording {} started for file {:?}", recording_id, file_path);
        Ok(recording_id)
    }

    /// Stop an active recording
    pub async fn stop_recording(&self, recording_id: &str) -> Result<()> {
        let mut loggers = self.active_loggers.write().await;

        if let Some(handle) = loggers.remove(recording_id) {
            handle.abort_handle.abort();
            // Update ended_at in database
            let now = Utc::now().timestamp();
            self.database.recording_update_ended(recording_id, now)?;
            info!("[SessionLogger] Stopped recording {}", recording_id);
            Ok(())
        } else {
            Err(anyhow::anyhow!("Recording {} not found", recording_id))
        }
    }

    /// Check if a session is being recorded
    pub async fn is_recording(&self, session_id: &str) -> bool {
        let loggers = self.active_loggers.read().await;
        loggers.values().any(|h| h.session_id == session_id)
    }

    /// Get the recording ID for a session
    pub async fn get_recording_id(&self, session_id: &str) -> Option<String> {
        let loggers = self.active_loggers.read().await;
        loggers.values()
            .find(|h| h.session_id == session_id)
            .map(|h| h.recording_id.clone())
    }

    /// Stop all active recordings
    pub async fn stop_all(&self) {
        let mut loggers = self.active_loggers.write().await;
        let now = Utc::now().timestamp();

        for (id, handle) in loggers.drain() {
            handle.abort_handle.abort();
            let _ = self.database.recording_update_ended(&id, now);
            info!("[SessionLogger] Stopped recording {}", id);
        }
    }

    /// Stop all recordings for a specific session
    pub async fn stop_for_session(&self, session_id: &str) {
        let mut loggers = self.active_loggers.write().await;
        let now = Utc::now().timestamp();

        let to_remove: Vec<String> = loggers.iter()
            .filter(|(_, h)| h.session_id == session_id)
            .map(|(k, _)| k.clone())
            .collect();

        for id in to_remove {
            if let Some(handle) = loggers.remove(&id) {
                handle.abort_handle.abort();
                let _ = self.database.recording_update_ended(&id, now);
            }
        }
    }
}
