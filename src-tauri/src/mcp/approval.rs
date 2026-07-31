//! User-approval gating for risky agent commands.
//!
//! When a command is classified as dangerous, the tool handler calls
//! [`AgentApprovalManager::gate`], which suspends the in-flight MCP request
//! while a confirmation dialog is surfaced in the desktop GUI. The frontend
//! resolves the request through a Tauri command, unblocking the handler.
//!
//! The manager also owns the optional "auto-approve for N hours" window: while
//! active, dangerous commands are approved immediately (they remain visible in
//! the shared terminal). Its deadline is persisted by the desktop command layer
//! so the remainder of the five-hour grant survives an app restart.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tokio::sync::oneshot;
use uuid::Uuid;

/// Settings key used to preserve the five-hour grant across app restarts.
pub const AUTO_APPROVE_UNTIL_KEY: &str = "agent_auto_approve_until";

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Result of gating a command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalOutcome {
    Approved,
    Denied(String),
}

/// Input describing the command awaiting approval.
#[derive(Debug, Clone)]
pub struct ApprovalRequest {
    pub tool: String,
    pub command: String,
    pub reasons: Vec<String>,
    pub session_id: Option<String>,
}

/// Payload emitted to the GUI when a command needs approval.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRequestEvent {
    pub id: String,
    pub sequence: u64,
    pub tool: String,
    pub command: String,
    pub reasons: Vec<String>,
    pub session_id: Option<String>,
    pub timestamp: i64,
}

/// Payload emitted whenever the auto-approve window changes.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalStateEvent {
    /// Epoch milliseconds until which commands auto-approve, or `None`.
    pub auto_approve_until: Option<i64>,
}

/// Payload emitted when a pending request is no longer actionable.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalResolvedEvent {
    pub id: String,
}

/// Snapshot of the guard's runtime state for the GUI to query.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalStatus {
    pub auto_approve_until: Option<i64>,
    pub pending: Vec<ApprovalRequestEvent>,
}

/// Events the manager pushes to the desktop event bridge.
pub enum ApprovalEvent {
    Request(ApprovalRequestEvent),
    Resolved(ApprovalResolvedEvent),
    State(ApprovalStateEvent),
}

struct PendingApproval {
    sender: oneshot::Sender<ApprovalOutcome>,
    event: ApprovalRequestEvent,
}

/// Coordinates approval round-trips and the auto-approve window.
pub struct AgentApprovalManager {
    pending: Mutex<HashMap<String, PendingApproval>>,
    auto_approve_until: Mutex<Option<i64>>,
    emit: Arc<dyn Fn(ApprovalEvent) + Send + Sync>,
    timeout: Option<Duration>,
    next_sequence: AtomicU64,
}

impl AgentApprovalManager {
    pub fn new(emit: Arc<dyn Fn(ApprovalEvent) + Send + Sync>) -> Self {
        Self::with_auto_approve_until(emit, None)
    }

    /// Restore a persisted auto-approve deadline when it is still active.
    pub fn with_auto_approve_until(
        emit: Arc<dyn Fn(ApprovalEvent) + Send + Sync>,
        auto_approve_until: Option<i64>,
    ) -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
            auto_approve_until: Mutex::new(auto_approve_until.filter(|until| now_ms() < *until)),
            emit,
            // A confirmation request is intentionally durable for the app
            // lifetime: the command remains suspended until the user decides.
            timeout: None,
            next_sequence: AtomicU64::new(0),
        }
    }

    #[cfg(test)]
    fn with_timeout(emit: Arc<dyn Fn(ApprovalEvent) + Send + Sync>, timeout: Duration) -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
            auto_approve_until: Mutex::new(None),
            emit,
            timeout: Some(timeout),
            next_sequence: AtomicU64::new(0),
        }
    }

    /// Block until the command is approved or denied.
    ///
    /// Returns immediately with `Approved` when the auto-approve window is
    /// active. Otherwise emits a request event and waits for the GUI to resolve
    /// it. Production requests do not expire while the application is running.
    pub async fn gate(&self, request: ApprovalRequest) -> ApprovalOutcome {
        if self.auto_approve_active() {
            return ApprovalOutcome::Approved;
        }

        let id = Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        let event = ApprovalRequestEvent {
            id: id.clone(),
            sequence: self.next_sequence.fetch_add(1, Ordering::Relaxed),
            tool: request.tool,
            command: request.command,
            reasons: request.reasons,
            session_id: request.session_id,
            timestamp: now_ms(),
        };
        {
            let mut pending = self.pending.lock().expect("approval mutex poisoned");
            pending.insert(
                id.clone(),
                PendingApproval {
                    sender: tx,
                    event: event.clone(),
                },
            );
        }

        (self.emit)(ApprovalEvent::Request(event));
        let _cleanup = PendingApprovalCleanup {
            manager: self,
            id: id.clone(),
        };

        let received = if let Some(timeout) = self.timeout {
            match tokio::time::timeout(timeout, rx).await {
                Ok(received) => received,
                Err(_) => {
                    self.remove_and_emit(&id);
                    return ApprovalOutcome::Denied(format!(
                        "no response within {}s",
                        timeout.as_secs()
                    ));
                }
            }
        } else {
            rx.await
        };

        match received {
            Ok(outcome) => outcome,
            Err(_) => {
                self.remove_and_emit(&id);
                ApprovalOutcome::Denied("approval channel closed".to_string())
            }
        }
    }

    /// Resolve a pending approval, optionally opening an auto-approve window.
    pub fn resolve(
        &self,
        id: &str,
        outcome: ApprovalOutcome,
        auto_approve_ms: Option<i64>,
    ) -> bool {
        let (pending_request, queued) = {
            let mut pending = self.pending.lock().expect("approval mutex poisoned");
            let Some(pending_request) = pending.remove(id) else {
                return false;
            };

            let queued = if matches!(outcome, ApprovalOutcome::Approved)
                && auto_approve_ms.is_some_and(|ms| ms > 0)
            {
                pending.drain().collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            (pending_request, queued)
        };

        let opens_window = matches!(outcome, ApprovalOutcome::Approved)
            && auto_approve_ms.is_some_and(|ms| ms > 0);
        if opens_window {
            let ms = auto_approve_ms.expect("checked above");
            *self
                .auto_approve_until
                .lock()
                .expect("auto-approve mutex poisoned") = Some(now_ms().saturating_add(ms));
            self.emit_state();
        }

        let _ = pending_request.sender.send(outcome);
        self.emit_resolved(id);

        // Turning on auto-confirm also releases requests that were already
        // queued concurrently. Otherwise the UI would keep asking about work
        // that now falls inside the active auto-confirm window.
        for (queued_id, queued_request) in queued {
            let _ = queued_request.sender.send(ApprovalOutcome::Approved);
            self.emit_resolved(&queued_id);
        }

        true
    }

    /// Close the auto-approve window immediately.
    pub fn cancel_auto_approve(&self) {
        *self
            .auto_approve_until
            .lock()
            .expect("auto-approve mutex poisoned") = None;
        self.emit_state();
    }

    /// Current auto-approve deadline (only if still in the future).
    pub fn status(&self) -> ApprovalStatus {
        let mut pending = self
            .pending
            .lock()
            .expect("approval mutex poisoned")
            .values()
            .map(|request| request.event.clone())
            .collect::<Vec<_>>();
        pending.sort_by_key(|request| (request.timestamp, request.sequence));
        ApprovalStatus {
            auto_approve_until: self.active_deadline(),
            pending,
        }
    }

    fn remove_and_emit(&self, id: &str) {
        let removed = self
            .pending
            .lock()
            .expect("approval mutex poisoned")
            .remove(id)
            .is_some();
        if removed {
            self.emit_resolved(id);
        }
    }

    fn auto_approve_active(&self) -> bool {
        self.active_deadline().is_some()
    }

    fn active_deadline(&self) -> Option<i64> {
        self.auto_approve_until
            .lock()
            .expect("auto-approve mutex poisoned")
            .filter(|until| now_ms() < *until)
    }

    fn emit_state(&self) {
        (self.emit)(ApprovalEvent::State(ApprovalStateEvent {
            auto_approve_until: self.active_deadline(),
        }));
    }

    fn emit_resolved(&self, id: &str) {
        (self.emit)(ApprovalEvent::Resolved(ApprovalResolvedEvent {
            id: id.to_string(),
        }));
    }
}

/// Removes a request if its in-flight MCP future is canceled before the
/// approval receiver completes. Normal resolution removes it first, making
/// this guard a no-op on the successful path.
struct PendingApprovalCleanup<'a> {
    manager: &'a AgentApprovalManager,
    id: String,
}

impl Drop for PendingApprovalCleanup<'_> {
    fn drop(&mut self) {
        self.manager.remove_and_emit(&self.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    /// Build a manager whose emitted request ids are forwarded on a channel so
    /// tests can resolve without racing the insert.
    fn manager_with_id_channel(
        timeout: Duration,
    ) -> (Arc<AgentApprovalManager>, mpsc::Receiver<String>) {
        let (id_tx, id_rx) = mpsc::channel::<String>();
        let emit = Arc::new(move |event: ApprovalEvent| {
            if let ApprovalEvent::Request(request) = event {
                let _ = id_tx.send(request.id);
            }
        });
        (
            Arc::new(AgentApprovalManager::with_timeout(emit, timeout)),
            id_rx,
        )
    }

    #[test]
    fn resolves_approved_and_denied() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let (manager, id_rx) = manager_with_id_channel(Duration::from_secs(5));

            let gate_manager = manager.clone();
            let handle = tokio::spawn(async move {
                gate_manager
                    .gate(ApprovalRequest {
                        tool: "exec".to_string(),
                        command: "rm -rf /tmp/x".to_string(),
                        reasons: vec!["dangerous".to_string()],
                        session_id: Some("s1".to_string()),
                    })
                    .await
            });

            let id = tokio::task::spawn_blocking(move || id_rx.recv().unwrap())
                .await
                .unwrap();
            assert!(manager.resolve(&id, ApprovalOutcome::Approved, None));
            assert_eq!(handle.await.unwrap(), ApprovalOutcome::Approved);
        });
    }

    #[test]
    fn times_out_into_denial() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let (manager, _id_rx) = manager_with_id_channel(Duration::from_millis(50));
            let outcome = manager
                .gate(ApprovalRequest {
                    tool: "exec".to_string(),
                    command: "rm -rf /".to_string(),
                    reasons: vec![],
                    session_id: None,
                })
                .await;
            assert!(matches!(outcome, ApprovalOutcome::Denied(_)));
        });
    }

    #[test]
    fn auto_approve_window_bypasses_gate() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let (manager, _id_rx) = manager_with_id_channel(Duration::from_millis(50));
            // Open a window without a pending id.
            let gate_manager = manager.clone();
            let handle = tokio::spawn(async move {
                gate_manager
                    .gate(ApprovalRequest {
                        tool: "exec".to_string(),
                        command: "rm -rf /tmp".to_string(),
                        reasons: vec![],
                        session_id: None,
                    })
                    .await
            });
            let id = tokio::task::spawn_blocking(move || _id_rx.recv().unwrap())
                .await
                .unwrap();
            assert!(manager.resolve(&id, ApprovalOutcome::Approved, Some(3_600_000)));
            assert_eq!(handle.await.unwrap(), ApprovalOutcome::Approved);
            assert!(manager.status().auto_approve_until.is_some());

            let outcome = manager
                .gate(ApprovalRequest {
                    tool: "exec".to_string(),
                    command: "rm -rf /tmp".to_string(),
                    reasons: vec![],
                    session_id: None,
                })
                .await;
            assert_eq!(outcome, ApprovalOutcome::Approved);

            manager.cancel_auto_approve();
            assert!(manager.status().auto_approve_until.is_none());
        });
    }

    #[test]
    fn unknown_request_cannot_enable_auto_approve() {
        let (manager, _id_rx) = manager_with_id_channel(Duration::from_secs(5));

        assert!(!manager.resolve("not-pending", ApprovalOutcome::Approved, Some(3_600_000),));
        assert!(manager.status().auto_approve_until.is_none());
    }

    #[test]
    fn restores_only_an_active_persisted_auto_approve_deadline() {
        let emit = Arc::new(|_event: ApprovalEvent| {});
        let future = now_ms() + 60_000;
        let active = AgentApprovalManager::with_auto_approve_until(emit.clone(), Some(future));
        assert_eq!(active.status().auto_approve_until, Some(future));

        let expired = AgentApprovalManager::with_auto_approve_until(emit, Some(now_ms() - 1));
        assert!(expired.status().auto_approve_until.is_none());
    }

    #[test]
    fn auto_approve_releases_requests_already_waiting() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let (manager, id_rx) = manager_with_id_channel(Duration::from_secs(5));
            let mut handles = Vec::new();
            for command in ["rm -rf /tmp/a", "rm -rf /tmp/b"] {
                let gate_manager = manager.clone();
                handles.push(tokio::spawn(async move {
                    gate_manager
                        .gate(ApprovalRequest {
                            tool: "exec".to_string(),
                            command: command.to_string(),
                            reasons: vec!["dangerous".to_string()],
                            session_id: Some("s1".to_string()),
                        })
                        .await
                }));
            }

            let ids = tokio::task::spawn_blocking(move || {
                vec![id_rx.recv().unwrap(), id_rx.recv().unwrap()]
            })
            .await
            .unwrap();
            assert!(manager.resolve(&ids[0], ApprovalOutcome::Approved, Some(3_600_000),));

            for handle in handles {
                assert_eq!(handle.await.unwrap(), ApprovalOutcome::Approved);
            }
        });
    }

    #[test]
    fn canceling_a_gate_removes_its_pending_snapshot() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let (manager, id_rx) = manager_with_id_channel(Duration::from_secs(5));
            let gate_manager = manager.clone();
            let handle = tokio::spawn(async move {
                gate_manager
                    .gate(ApprovalRequest {
                        tool: "exec".to_string(),
                        command: "rm -rf /tmp/x".to_string(),
                        reasons: vec!["dangerous".to_string()],
                        session_id: Some("s1".to_string()),
                    })
                    .await
            });

            let _id = tokio::task::spawn_blocking(move || id_rx.recv().unwrap())
                .await
                .unwrap();
            assert_eq!(manager.status().pending.len(), 1);

            handle.abort();
            let _ = handle.await;
            assert!(manager.status().pending.is_empty());
        });
    }
}
