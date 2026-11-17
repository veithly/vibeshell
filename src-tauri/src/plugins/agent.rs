//! Shared plugin API for native CLI, MCP and the desktop.
use super::*;
use crate::{commands::plugin as host, session::SessionManager, storage::Database};
use serde_json::{json, Value};
use std::time::Instant;

pub fn list(db: &Database, installed_only: bool) -> Result<Vec<Value>, String> {
    Ok(host::list_plugins(db)?.into_iter()
        .filter(|record| !installed_only || record.installed)
        .map(|record| json!({"id": record.manifest.id, "name": record.manifest.name,
            "version": record.manifest.version, "description": record.manifest.description,
            "installed": record.installed, "enabled": record.enabled, "source": record.source,
            "permissions": record.manifest.permissions, "grantedPermissions": record.granted_permissions,
            "sessionTypes": record.manifest.session_types,
            "reference": format!("vibeshell plugins docs {}", record.manifest.id),
            "describe": format!("vibeshell plugins describe {}", record.manifest.id)})).collect())
}

pub fn describe(db: &Database, id: &str, reference: bool) -> Result<Value, String> {
    let record = host::list_plugins(db)?
        .into_iter()
        .find(|record| record.manifest.id == id)
        .ok_or_else(|| format!("Plugin not found: {id}"))?;
    if reference {
        return Ok(Value::String(agent_reference(&record.manifest)?));
    }
    Ok(
        json!({"id": id, "name": record.manifest.name, "version": record.manifest.version,
        "installed": record.installed, "enabled": record.enabled, "source": record.source,
        "permissions": record.manifest.permissions, "grantedPermissions": record.granted_permissions,
        "sessionTypes": record.manifest.session_types, "actions": agent_actions(&record.manifest),
        "reference": format!("vibeshell plugins docs {id}")}),
    )
}

pub struct PreparedAction {
    pub command: String,
    pub stdin: Option<String>,
    pub requires_confirmation: bool,
    pub native_status: bool,
}

/// Pure preparation never executes, grants permissions or consumes confirmation.
pub fn prepare(
    db: &Database,
    request: &PluginExecuteRequest,
    is_local: bool,
) -> Result<PreparedAction, String> {
    let installation = db
        .plugin_installation_get(&request.plugin_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Plugin is not installed: {}", request.plugin_id))?;
    if !installation.enabled {
        return Err(format!("Plugin is disabled: {}", request.plugin_id));
    }
    let source = PluginSource::parse(&installation.source)?;
    let manifest = host::manifest_for_installation(&installation, &source)?;
    let target = if is_local {
        PluginSessionType::Local
    } else {
        PluginSessionType::Ssh
    };
    if !manifest.session_types.contains(&target) {
        return Err("Plugin does not support this session type".into());
    }
    let granted: Vec<PluginPermission> =
        serde_json::from_str(&installation.granted_permissions_json)
            .map_err(|_| "Stored plugin permissions are invalid".to_string())?;
    let native_status =
        matches!(&manifest.entry, PluginEntry::Native { view } if view == "server-status");
    let permission = if native_status {
        PluginPermission::LocalSystemRead
    } else if is_local {
        PluginPermission::LocalExec
    } else {
        PluginPermission::RemoteExec
    };
    if !manifest.permissions.contains(&permission)
        || !host::permission_satisfied(source, &manifest, &granted, &permission)
    {
        return Err("Plugin lacks the required declared and granted permission".into());
    }
    if native_status {
        if request.action_id != "status"
            || !request.inputs.is_empty()
            || request.try_sudo
            || request.sudo_password.is_some()
        {
            return Err("Server Performance exposes only status with no inputs or sudo".into());
        }
        return Ok(PreparedAction {
            command: crate::commands::session::REMOTE_STATUS_COMMAND.into(),
            stdin: None,
            requires_confirmation: false,
            native_status: true,
        });
    }
    let PluginEntry::Commands { actions } = manifest.entry else {
        return Err("Unsupported native plugin".into());
    };
    let action = actions
        .into_iter()
        .find(|action| action.id == request.action_id)
        .ok_or_else(|| {
            format!(
                "Plugin action not found: {}/{}",
                request.plugin_id, request.action_id
            )
        })?;
    if request.try_sudo && !action.allow_sudo {
        return Err("This action does not allow optional sudo".into());
    }
    let use_sudo = action.elevate || request.try_sudo;
    let stdin = if use_sudo {
        request
            .sudo_password
            .clone()
            .filter(|value| !value.is_empty())
    } else {
        None
    };
    let command = render_command(&action, &request.inputs, use_sudo, stdin.is_some())?;
    let config = crate::mcp::GuardConfig::from_stored_json(
        db.get_setting(crate::mcp::guard::GUARD_CONFIG_KEY)
            .map_err(|e| e.to_string())?
            .as_deref(),
    );
    let requires_confirmation = action.requires_confirmation
        || use_sudo
        || (config.enabled
            && crate::mcp::guard::classify_command(&command, &config).requires_approval);
    Ok(PreparedAction {
        command,
        stdin,
        requires_confirmation,
        native_status: false,
    })
}

pub async fn is_local(manager: &SessionManager, session_id: &str) -> bool {
    match manager.local_shell_manager.get() {
        Some(local) => local.get_session(session_id).await.is_some(),
        None => false,
    }
}

pub async fn execute(
    db: &Database,
    manager: &SessionManager,
    request: PluginExecuteRequest,
    source: &str,
    reviewed_command: Option<&str>,
) -> Result<PluginExecutionResult, String> {
    use crate::mcp::server::{AgentActivityEvent, AgentActivityStatus};
    let local = is_local(manager, &request.session_id).await;
    let prepared = prepare(db, &request, local)?;
    if reviewed_command.is_some_and(|reviewed| reviewed != prepared.command) {
        return Err(
            "Plugin changed while approval was pending; review the current action again".into(),
        );
    }
    if prepared.requires_confirmation && !request.confirmed {
        return Err("This plugin action requires explicit approval; request human confirmation before using --confirm".into());
    }
    let mut event = AgentActivityEvent {
        id: uuid::Uuid::new_v4().to_string(),
        tool: source.into(),
        session_id: Some(request.session_id.clone()),
        summary: format!(
            "{}/{}\n{}",
            request.plugin_id,
            request.action_id,
            if prepared.native_status {
                "Read performance snapshot"
            } else {
                &prepared.command
            }
        ),
        status: AgentActivityStatus::Started,
        timestamp: chrono::Utc::now().timestamp_millis(),
    };
    db.agent_activity_record(&event)
        .map_err(|e| e.to_string())?;
    let started = Instant::now();
    let outcome: Result<String, String> = async {
        if local {
            if prepared.native_status {
                let status = tokio::task::spawn_blocking(
                    crate::commands::session::collect_local_server_status,
                )
                .await
                .map_err(|e| e.to_string())?;
                return serde_json::to_string(&status).map_err(|e| e.to_string());
            }
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            {
                return host::run_local_command(&prepared.command, prepared.stdin.as_deref()).await;
            }
            #[cfg(any(target_os = "android", target_os = "ios"))]
            {
                return Err("Local plugins are unavailable on mobile".into());
            }
        }
        let session = manager
            .get(&request.session_id)
            .await
            .ok_or("Session not found")?;
        // The SSH client bounds output and timeout without piping through head,
        // which would mask the action's failure exit status.
        let output = session
            .exec_command_with_stdin(&prepared.command, prepared.stdin.as_deref())
            .await
            .map_err(|e| e.to_string())?;
        if prepared.native_status {
            return serde_json::to_string(&crate::commands::session::parse_remote_server_status(
                &output,
            )?)
            .map_err(|e| e.to_string());
        }
        Ok(output)
    }
    .await;
    event.status = if outcome.is_ok() {
        AgentActivityStatus::Succeeded
    } else {
        AgentActivityStatus::Failed
    };
    event.timestamp = chrono::Utc::now().timestamp_millis();
    if let Err(error) = db.agent_activity_record(&event) {
        log::error!("Plugin completion audit failed: {error}");
    }
    let (output, truncated) = host::truncate_output(outcome?);
    Ok(PluginExecutionResult {
        plugin_id: request.plugin_id,
        action_id: request.action_id,
        output,
        truncated,
        duration_ms: started.elapsed().as_millis().min(u64::MAX as u128) as u64,
    })
}
