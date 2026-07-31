//! Tauri commands backing the agent command-guard UI (approval dialog,
//! auto-approve window, and the persisted risk ruleset).

use std::sync::Arc;

use serde::Deserialize;
use tauri::State;

use crate::mcp::approval::{ApprovalStatus, AUTO_APPROVE_UNTIL_KEY};
use crate::mcp::guard::{GuardConfig, GUARD_CONFIG_KEY};
use crate::storage::Database;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
use crate::mcp::{AgentApprovalManager, ApprovalOutcome};

const MAX_AUTO_APPROVE_HOURS: f64 = 5.0;

fn auto_approve_ms(hours: Option<f64>) -> Result<Option<i64>, String> {
    let Some(hours) = hours else {
        return Ok(None);
    };
    if !hours.is_finite() || hours <= 0.0 || hours > MAX_AUTO_APPROVE_HOURS {
        return Err(format!(
            "autoApproveHours must be greater than 0 and at most {MAX_AUTO_APPROVE_HOURS}"
        ));
    }

    Ok(Some((hours * 3_600_000.0).round() as i64))
}

/// Decision payload sent from the approval dialog.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveApprovalRequest {
    /// Id of the pending approval to resolve.
    pub id: String,
    /// Whether the command was approved.
    pub approved: bool,
    /// When approved, open an auto-approve window of this many hours.
    #[serde(default)]
    pub auto_approve_hours: Option<f64>,
}

/// Resolve a pending approval and optionally open an auto-approve window.
#[tauri::command]
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn resolve_agent_approval(
    approvals: State<'_, Arc<AgentApprovalManager>>,
    db: State<'_, Arc<Database>>,
    request: ResolveApprovalRequest,
) -> Result<(), String> {
    let auto_ms = if request.approved {
        auto_approve_ms(request.auto_approve_hours)?
    } else {
        None
    };
    let outcome = if request.approved {
        ApprovalOutcome::Approved
    } else {
        ApprovalOutcome::Denied("Denied by user".to_string())
    };
    if approvals.resolve(&request.id, outcome, auto_ms) {
        if auto_ms.is_some() {
            if let Some(until) = approvals.status().auto_approve_until {
                if let Err(error) = db.set_setting(AUTO_APPROVE_UNTIL_KEY, &until.to_string()) {
                    log::warn!("Could not persist agent auto-approve deadline: {}", error);
                }
            }
        }
        Ok(())
    } else {
        Err("Approval request is no longer pending".to_string())
    }
}

/// Mobile builds have no Agent Gateway, so approvals never fire.
#[tauri::command]
#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn resolve_agent_approval(_request: ResolveApprovalRequest) -> Result<(), String> {
    Ok(())
}

/// Report the current auto-approve deadline (if any).
#[tauri::command]
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn get_agent_guard_status(approvals: State<'_, Arc<AgentApprovalManager>>) -> ApprovalStatus {
    approvals.status()
}

#[tauri::command]
#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn get_agent_guard_status() -> ApprovalStatus {
    ApprovalStatus {
        auto_approve_until: None,
        pending: Vec::new(),
    }
}

/// Close the auto-approve window immediately.
#[tauri::command]
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn cancel_agent_auto_approve(
    approvals: State<'_, Arc<AgentApprovalManager>>,
    db: State<'_, Arc<Database>>,
) -> Result<(), String> {
    db.set_setting(AUTO_APPROVE_UNTIL_KEY, "0")
        .map_err(|error| error.to_string())?;
    approvals.cancel_auto_approve();
    Ok(())
}

#[tauri::command]
#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn cancel_agent_auto_approve() -> Result<(), String> {
    Ok(())
}

/// Read the persisted command-guard configuration.
#[tauri::command]
pub fn get_agent_guard_config(db: State<'_, Arc<Database>>) -> Result<GuardConfig, String> {
    let json = db
        .get_setting(GUARD_CONFIG_KEY)
        .map_err(|e| e.to_string())?;
    Ok(GuardConfig::from_stored_json(json.as_deref()))
}

/// Persist the command-guard configuration.
#[tauri::command]
pub fn set_agent_guard_config(
    db: State<'_, Arc<Database>>,
    config: GuardConfig,
) -> Result<(), String> {
    let json = serde_json::to_string(&config).map_err(|e| e.to_string())?;
    db.set_setting(GUARD_CONFIG_KEY, &json)
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_approve_duration_is_bounded_and_finite() {
        assert_eq!(auto_approve_ms(None).unwrap(), None);
        assert_eq!(auto_approve_ms(Some(5.0)).unwrap(), Some(18_000_000));
        assert!(auto_approve_ms(Some(5.01)).is_err());
        assert!(auto_approve_ms(Some(0.0)).is_err());
        assert!(auto_approve_ms(Some(f64::NAN)).is_err());
        assert!(auto_approve_ms(Some(f64::INFINITY)).is_err());
    }
}
