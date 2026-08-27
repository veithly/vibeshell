use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const MAX_MANIFEST_BYTES: usize = 256 * 1024;
pub const MAX_PLUGIN_OUTPUT_BYTES: usize = 1_000_000;
pub const MAX_PLUGIN_SETTINGS_BYTES: usize = 16 * 1024;

/// The built-in plugin catalog. Each plugin lives in its own directory under
/// `builtin/` and is embedded at compile time; the pairing test below fails if
/// a directory is added without registering it here (or vice versa).
const BUILTIN_MANIFESTS: &[(&str, &str)] = &[
    (
        "server-performance",
        include_str!("../builtin/server-performance/plugin.json"),
    ),
    (
        "docker-containers",
        include_str!("../builtin/docker-containers/plugin.json"),
    ),
    (
        "kubernetes-pods",
        include_str!("../builtin/kubernetes-pods/plugin.json"),
    ),
    (
        "database-inspector",
        include_str!("../builtin/database-inspector/plugin.json"),
    ),
    (
        "redis-inspector",
        include_str!("../builtin/redis-inspector/plugin.json"),
    ),
    (
        "process-explorer",
        include_str!("../builtin/process-explorer/plugin.json"),
    ),
    (
        "system-logs",
        include_str!("../builtin/system-logs/plugin.json"),
    ),
    (
        "network-inspector",
        include_str!("../builtin/network-inspector/plugin.json"),
    ),
    (
        "disk-usage",
        include_str!("../builtin/disk-usage/plugin.json"),
    ),
    (
        "git-workspace",
        include_str!("../builtin/git-workspace/plugin.json"),
    ),
    (
        "cron-scheduler",
        include_str!("../builtin/cron-scheduler/plugin.json"),
    ),
    (
        "systemd-services",
        include_str!("../builtin/systemd-services/plugin.json"),
    ),
];

/// POSIX shell quoting for a single argument. Values that contain anything
/// beyond the safe alphanumeric/punctuation set are wrapped in single quotes
/// with embedded quotes escaped; this is the boundary that keeps a plugin
/// input from breaking out of its argument.
pub fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }

    if value.chars().all(|ch| {
        ch.is_ascii_alphanumeric()
            || matches!(
                ch,
                '_' | '@' | '%' | '+' | '=' | ':' | ',' | '.' | '/' | '-'
            )
    }) {
        return value.to_string();
    }

    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum PluginPermission {
    RemoteExec,
    LocalSystemRead,
    LocalExec,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginSessionType {
    Ssh,
    Local,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginSource {
    Builtin,
    External,
}

impl PluginSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Builtin => "builtin",
            Self::External => "external",
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "builtin" => Ok(Self::Builtin),
            "external" => Ok(Self::External),
            _ => Err(format!("Unknown plugin source: {}", value)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestValidationPolicy {
    Builtin,
    External,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub author: String,
    pub category: String,
    pub icon: String,
    #[serde(default)]
    pub permissions: Vec<PluginPermission>,
    pub session_types: Vec<PluginSessionType>,
    #[serde(default = "empty_object")]
    pub default_settings: Value,
    pub entry: PluginEntry,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PluginEntry {
    Native { view: String },
    Commands { actions: Vec<PluginAction> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginAction {
    pub id: String,
    pub name: String,
    pub description: String,
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub inputs: Vec<PluginInput>,
    #[serde(default)]
    pub requires_confirmation: bool,
    /// When true the command runs under `sudo` so privileged actions (docker
    /// restart, systemctl, etc.) work without a dedicated interactive shell.
    #[serde(default)]
    pub elevate: bool,
    /// Allows the user to explicitly retry this action with `sudo` while the
    /// default execution path remains unprivileged.
    #[serde(default)]
    pub allow_sudo: bool,
    #[serde(default)]
    pub output: PluginOutput,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginInput {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub placeholder: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub kind: PluginInputKind,
    #[serde(default)]
    pub options: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PluginInputKind {
    #[default]
    Text,
    Integer,
    Boolean,
    Select,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginOutput {
    #[serde(default)]
    pub kind: PluginOutputKind,
    #[serde(default)]
    pub columns: Vec<String>,
    #[serde(default = "default_delimiter")]
    pub delimiter: String,
}

impl Default for PluginOutput {
    fn default() -> Self {
        Self {
            kind: PluginOutputKind::Text,
            columns: Vec::new(),
            delimiter: default_delimiter(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginOutputKind {
    #[default]
    Text,
    Table,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginRecord {
    pub manifest: PluginManifest,
    pub source: PluginSource,
    pub installed: bool,
    pub enabled: bool,
    pub granted_permissions: Vec<PluginPermission>,
    pub settings: Value,
    pub installed_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginExecuteRequest {
    pub plugin_id: String,
    pub action_id: String,
    pub session_id: String,
    #[serde(default)]
    pub inputs: BTreeMap<String, Value>,
    /// Optional sudo password for elevated actions. Lives in memory only and is
    /// never persisted. When omitted the action falls back to `sudo -n`
    /// (non-interactive), which works when the host is configured NOPASSWD.
    #[serde(default)]
    pub sudo_password: Option<String>,
    /// Explicitly use sudo for an action that declares `allowSudo`.
    #[serde(default)]
    pub try_sudo: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginExecutionResult {
    pub plugin_id: String,
    pub action_id: String,
    pub output: String,
    pub duration_ms: u64,
    pub truncated: bool,
}

fn empty_object() -> Value {
    Value::Object(Default::default())
}

fn default_delimiter() -> String {
    "\t".to_string()
}

pub fn builtin_catalog() -> Result<Vec<PluginManifest>, String> {
    BUILTIN_MANIFESTS
        .iter()
        .map(|(id, json)| {
            let manifest: PluginManifest = serde_json::from_str(json)
                .map_err(|error| format!("Built-in plugin manifest {id} is invalid: {error}"))?;
            if manifest.id != *id {
                return Err(format!(
                    "Built-in plugin directory {id} contains manifest id {}",
                    manifest.id
                ));
            }
            validate_manifest(&manifest, ManifestValidationPolicy::Builtin)?;
            Ok(manifest)
        })
        .collect()
}

pub fn parse_manifest(
    json: &str,
    policy: ManifestValidationPolicy,
) -> Result<PluginManifest, String> {
    if json.len() > MAX_MANIFEST_BYTES {
        return Err(format!(
            "Plugin manifest exceeds the {} KB limit",
            MAX_MANIFEST_BYTES / 1024
        ));
    }

    let manifest: PluginManifest = serde_json::from_str(json)
        .map_err(|error| format!("Invalid plugin manifest JSON: {}", error))?;
    validate_manifest(&manifest, policy)?;
    Ok(manifest)
}

pub fn validate_manifest(
    manifest: &PluginManifest,
    policy: ManifestValidationPolicy,
) -> Result<(), String> {
    if manifest.schema_version != 1 {
        return Err(format!(
            "Unsupported plugin schema version: {}",
            manifest.schema_version
        ));
    }
    validate_identifier(&manifest.id, "plugin id")?;
    validate_text(&manifest.name, "name", 80)?;
    validate_text(&manifest.description, "description", 600)?;
    validate_text(&manifest.version, "version", 32)?;
    validate_text(&manifest.author, "author", 80)?;
    validate_identifier(&manifest.category, "category")?;
    validate_identifier(&manifest.icon, "icon")?;

    if manifest.session_types.is_empty() {
        return Err("Plugin must support at least one session type".to_string());
    }
    if !manifest.default_settings.is_object() {
        return Err("defaultSettings must be a JSON object".to_string());
    }
    let settings_size = serde_json::to_vec(&manifest.default_settings)
        .map_err(|error| format!("Failed to encode defaultSettings: {}", error))?
        .len();
    if settings_size > MAX_PLUGIN_SETTINGS_BYTES {
        return Err(format!(
            "defaultSettings exceeds the {} KB limit",
            MAX_PLUGIN_SETTINGS_BYTES / 1024
        ));
    }

    let mut permissions = HashSet::new();
    for permission in &manifest.permissions {
        if !permissions.insert(permission) {
            return Err("Plugin permissions must not contain duplicates".to_string());
        }
    }
    if policy == ManifestValidationPolicy::External
        && manifest.permissions.iter().any(|permission| {
            !matches!(
                permission,
                PluginPermission::RemoteExec | PluginPermission::LocalExec
            )
        })
    {
        return Err(
            "External plugins may only request remote_exec or local_exec permission".to_string(),
        );
    }

    match &manifest.entry {
        PluginEntry::Native { view } => {
            if policy == ManifestValidationPolicy::External {
                return Err("External plugins cannot declare native views".to_string());
            }
            if view != "server-status" {
                return Err(format!("Unknown native plugin view: {}", view));
            }
        }
        PluginEntry::Commands { actions } => {
            let needs_remote = manifest.session_types.contains(&PluginSessionType::Ssh);
            let needs_local = manifest.session_types.contains(&PluginSessionType::Local);
            if needs_remote && !manifest.permissions.contains(&PluginPermission::RemoteExec) {
                return Err(
                    "Command plugins supporting SSH sessions must request remote_exec permission"
                        .to_string(),
                );
            }
            if needs_local && !manifest.permissions.contains(&PluginPermission::LocalExec) {
                return Err(
                    "Command plugins supporting local sessions must request local_exec permission"
                        .to_string(),
                );
            }
            if !needs_remote && !needs_local {
                return Err("Command plugins must support at least one executable session type (ssh or local)".to_string());
            }
            if actions.is_empty() || actions.len() > 24 {
                return Err("Command plugins must declare between 1 and 24 actions".to_string());
            }

            let mut action_ids = HashSet::new();
            for action in actions {
                validate_action(action)?;
                if !action_ids.insert(&action.id) {
                    return Err(format!("Duplicate plugin action id: {}", action.id));
                }
            }
        }
    }

    Ok(())
}

fn validate_action(action: &PluginAction) -> Result<(), String> {
    validate_identifier(&action.id, "action id")?;
    validate_text(&action.name, "action name", 80)?;
    validate_text(&action.description, "action description", 300)?;
    validate_program(&action.program)?;

    // Elevated actions must require confirmation so the user always opts in
    // before a privileged command runs.
    if action.elevate && !action.requires_confirmation {
        return Err(format!(
            "Action {} requests elevation and must set requiresConfirmation",
            action.id
        ));
    }
    if action.elevate && action.allow_sudo {
        return Err(format!(
            "Action {} cannot require sudo and offer optional sudo at the same time",
            action.id
        ));
    }

    if action.args.len() > 32 {
        return Err(format!("Action {} has too many arguments", action.id));
    }
    if action.inputs.len() > 8 {
        return Err(format!("Action {} has too many inputs", action.id));
    }

    let mut input_ids = HashSet::new();
    for input in &action.inputs {
        validate_identifier(&input.id, "input id")?;
        validate_text(&input.label, "input label", 80)?;
        validate_optional_text(&input.description, "input description", 240)?;
        validate_optional_text(&input.placeholder, "input placeholder", 120)?;
        if !input_ids.insert(input.id.as_str()) {
            return Err(format!("Duplicate input id: {}", input.id));
        }
        if matches!(input.kind, PluginInputKind::Select) && input.options.is_empty() {
            return Err(format!("Select input {} must declare options", input.id));
        }
        if input.options.len() > 32 {
            return Err(format!("Input {} has too many options", input.id));
        }
        if !matches!(input.kind, PluginInputKind::Select) && !input.options.is_empty() {
            return Err(format!(
                "Only select input {} may declare options",
                input.id
            ));
        }
        let mut options = HashSet::new();
        for option in &input.options {
            validate_text(option, "input option", 80)?;
            if !options.insert(option) {
                return Err(format!("Input {} contains duplicate options", input.id));
            }
        }
    }

    for arg in &action.args {
        if arg.len() > 2048 {
            return Err(format!(
                "Action {} contains an oversized argument",
                action.id
            ));
        }
        let references = referenced_inputs(arg)?;
        if let Some(referenced) = references.first() {
            let exact_template = format!("{{{{input.{}}}}}", referenced);
            if references.len() != 1 || arg != &exact_template {
                return Err(format!(
                    "Action {} input templates must occupy a complete argument",
                    action.id
                ));
            }
        }
        for referenced in references {
            if !input_ids.contains(referenced.as_str()) {
                return Err(format!(
                    "Action {} references undeclared input {}",
                    action.id, referenced
                ));
            }
        }
    }

    if action.output.kind == PluginOutputKind::Table {
        if action.output.columns.is_empty() || action.output.columns.len() > 16 {
            return Err(format!(
                "Table action {} must declare between 1 and 16 columns",
                action.id
            ));
        }
        if action.output.delimiter.is_empty()
            || action.output.delimiter.len() > 4
            || action.output.delimiter.contains(['\n', '\r'])
        {
            return Err(format!(
                "Action {} has an invalid table delimiter",
                action.id
            ));
        }
    }

    Ok(())
}

fn validate_identifier(value: &str, field: &str) -> Result<(), String> {
    let valid = (3..=64).contains(&value.len())
        && value
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit())
        && value.chars().all(|ch| {
            ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '.' | '_' | '-')
        });

    if valid {
        Ok(())
    } else {
        Err(format!("Invalid {}: {}", field, value))
    }
}

fn validate_text(value: &str, field: &str, max_len: usize) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > max_len || value.contains('\0') {
        Err(format!("Invalid plugin {}", field))
    } else {
        Ok(())
    }
}

fn validate_optional_text(value: &str, field: &str, max_len: usize) -> Result<(), String> {
    if value.len() > max_len || value.contains('\0') {
        Err(format!("Invalid plugin {}", field))
    } else {
        Ok(())
    }
}

fn validate_program(program: &str) -> Result<(), String> {
    let valid = !program.is_empty()
        && program.len() <= 256
        && !program.contains("..")
        && program
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '/' | '+'));

    if valid {
        Ok(())
    } else {
        Err(format!("Invalid plugin program: {}", program))
    }
}

fn referenced_inputs(template: &str) -> Result<Vec<String>, String> {
    let mut rest = template;
    let mut referenced = Vec::new();

    while let Some(start) = rest.find("{{input.") {
        let after_start = &rest[start + "{{input.".len()..];
        let end = after_start
            .find("}}")
            .ok_or_else(|| "Unclosed plugin input template".to_string())?;
        let input_id = &after_start[..end];
        validate_identifier(input_id, "input reference")?;
        referenced.push(input_id.to_string());
        rest = &after_start[end + 2..];
    }

    Ok(referenced)
}

pub fn render_remote_command(
    action: &PluginAction,
    inputs: &BTreeMap<String, Value>,
) -> Result<String, String> {
    // Legacy entry point preserved for callers that don't supply a sudo
    // password; elevated actions render as `sudo -n` (NOPASSWD).
    render_command(action, inputs, action.elevate, false)
}

/// Render an action's command, optionally prefixed with sudo. `use_sudo`
/// selects whether to elevate this invocation, while `has_password` selects
/// `sudo -S -p ''` or the non-interactive NOPASSWD-compatible `sudo -n` mode.
pub fn render_command(
    action: &PluginAction,
    inputs: &BTreeMap<String, Value>,
    use_sudo: bool,
    has_password: bool,
) -> Result<String, String> {
    let declared_ids: HashSet<&str> = action
        .inputs
        .iter()
        .map(|input| input.id.as_str())
        .collect();
    if let Some(extra) = inputs
        .keys()
        .find(|key| !declared_ids.contains(key.as_str()))
    {
        return Err(format!("Unexpected plugin input: {}", extra));
    }

    let mut values = BTreeMap::new();
    for input in &action.inputs {
        match inputs.get(&input.id) {
            Some(value) => {
                values.insert(input.id.as_str(), normalize_input(input, value)?);
            }
            None if input.required => {
                return Err(format!("Missing required plugin input: {}", input.id));
            }
            None => {
                values.insert(input.id.as_str(), String::new());
            }
        }
    }

    let mut command_parts = vec![shell_quote(&action.program)];
    for arg in &action.args {
        let references = referenced_inputs(arg)?;
        let rendered = match references.as_slice() {
            [] => arg.clone(),
            [input_id] if arg == &format!("{{{{input.{}}}}}", input_id) => values
                .get(input_id.as_str())
                .cloned()
                .ok_or_else(|| format!("Unresolved input template in action {}", action.id))?,
            _ => {
                return Err(format!(
                    "Action {} input templates must occupy a complete argument",
                    action.id
                ));
            }
        };
        command_parts.push(shell_quote(&rendered));
    }

    let body = command_parts.join(" ");
    if use_sudo {
        // `sudo -S -p ''` reads the password from stdin with an empty prompt so
        // no prompt text leaks into the captured output; the caller writes the
        // password to the exec channel's stdin. Without a password we use
        // `sudo -n` which fails immediately instead of hanging when NOPASSWD is
        // not configured.
        let prefix = if has_password {
            "sudo -S -p '' "
        } else {
            "sudo -n "
        };
        Ok(format!("{prefix}{body}"))
    } else {
        Ok(body)
    }
}

fn normalize_input(input: &PluginInput, value: &Value) -> Result<String, String> {
    let normalized = match input.kind {
        PluginInputKind::Text => value
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| format!("Input {} must be text", input.id))?,
        PluginInputKind::Integer => {
            if let Some(number) = value.as_i64() {
                number.to_string()
            } else if let Some(text) = value.as_str() {
                text.parse::<i64>()
                    .map(|number| number.to_string())
                    .map_err(|_| format!("Input {} must be an integer", input.id))?
            } else {
                return Err(format!("Input {} must be an integer", input.id));
            }
        }
        PluginInputKind::Boolean => value
            .as_bool()
            .map(|flag| flag.to_string())
            .ok_or_else(|| format!("Input {} must be true or false", input.id))?,
        PluginInputKind::Select => {
            let selected = value
                .as_str()
                .ok_or_else(|| format!("Input {} must be one of its options", input.id))?;
            if !input.options.iter().any(|option| option == selected) {
                return Err(format!("Input {} must be one of its options", input.id));
            }
            selected.to_string()
        }
    };

    if normalized.len() > 1024 || normalized.contains(['\0', '\n', '\r']) {
        return Err(format!(
            "Input {} contains unsupported characters",
            input.id
        ));
    }

    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn external_manifest(entry: PluginEntry) -> PluginManifest {
        PluginManifest {
            schema_version: 1,
            id: "example.tools".to_string(),
            name: "Example Tools".to_string(),
            description: "Example declarative plugin".to_string(),
            version: "1.0.0".to_string(),
            author: "Example".to_string(),
            category: "operations".to_string(),
            icon: "wrench".to_string(),
            permissions: vec![PluginPermission::RemoteExec],
            session_types: vec![PluginSessionType::Ssh],
            default_settings: empty_object(),
            entry,
        }
    }

    fn command_entry() -> PluginEntry {
        PluginEntry::Commands {
            actions: vec![PluginAction {
                id: "version".to_string(),
                name: "Version".to_string(),
                description: "Show the tool version".to_string(),
                program: "tool".to_string(),
                args: vec!["--version".to_string()],
                inputs: Vec::new(),
                requires_confirmation: false,
                elevate: false,
                allow_sudo: false,
                output: PluginOutput::default(),
            }],
        }
    }

    #[test]
    fn built_in_catalog_is_valid_and_unique() {
        let catalog = builtin_catalog().expect("built-in catalog should be valid");
        let ids: HashSet<&str> = catalog.iter().map(|plugin| plugin.id.as_str()).collect();
        assert_eq!(ids.len(), catalog.len());
        assert!(ids.contains("server-performance"));
        assert!(ids.contains("docker-containers"));
        assert!(ids.contains("kubernetes-pods"));
        assert!(ids.contains("database-inspector"));
        assert!(ids.contains("redis-inspector"));
        assert!(ids.contains("cron-scheduler"));
        assert!(ids.contains("systemd-services"));
    }

    #[test]
    fn docker_actions_offer_sudo_without_forcing_it() {
        let catalog = builtin_catalog().expect("built-in catalog should be valid");
        let docker = catalog
            .iter()
            .find(|plugin| plugin.id == "docker-containers")
            .expect("Docker plugin should be present");
        let PluginEntry::Commands { actions } = &docker.entry else {
            panic!("Docker plugin should expose command actions");
        };

        let action_ids: HashSet<&str> = actions.iter().map(|action| action.id.as_str()).collect();
        assert_eq!(
            action_ids,
            HashSet::from([
                "containers",
                "stats",
                "images",
                "logs",
                "inspect",
                "exec-command",
                "start-container",
                "stop-container",
                "restart-container",
                "volumes",
                "networks",
                "version",
            ])
        );
        assert!(actions.iter().all(|action| action.allow_sudo));
        assert!(actions.iter().all(|action| !action.elevate));

        let lifecycle = ["start-container", "stop-container", "restart-container"];
        for action_id in lifecycle {
            let action = actions
                .iter()
                .find(|action| action.id == action_id)
                .expect("lifecycle action should be present");
            assert!(action.requires_confirmation);
            assert_eq!(action.program, "docker");
            assert_eq!(
                action.args.first().map(String::as_str),
                action_id.strip_suffix("-container")
            );
        }

        let containers = actions
            .iter()
            .find(|action| action.id == "containers")
            .expect("Docker container list action should be present");
        let inputs = BTreeMap::new();
        assert!(render_command(containers, &inputs, false, false)
            .unwrap()
            .starts_with("docker ps"));
        assert!(render_command(containers, &inputs, true, false)
            .unwrap()
            .starts_with("sudo -n docker ps"));
    }

    #[test]
    fn redis_plugin_falls_back_to_docker_exec() {
        let catalog = builtin_catalog().expect("built-in catalog should be valid");
        let redis = catalog
            .iter()
            .find(|plugin| plugin.id == "redis-inspector")
            .expect("Redis plugin should be present");
        let PluginEntry::Commands { actions } = &redis.entry else {
            panic!("Redis plugin should expose command actions");
        };

        let ping = actions
            .iter()
            .find(|action| action.id == "ping")
            .expect("Redis ping action should be present");
        let rendered = render_command(ping, &BTreeMap::new(), false, false).unwrap();
        // Missing optional container renders as an empty `$0` argument so the
        // wrapper can auto-detect, and the trailing PING goes through "$@".
        assert!(rendered.starts_with("sh -c "));
        assert!(rendered.contains("command -v redis-cli"));
        assert!(rendered.contains("docker exec"));
        assert!(rendered.ends_with("'' PING"));

        let pinned = BTreeMap::from([(
            "container".to_string(),
            Value::String("cache 1".to_string()),
        )]);
        let rendered = render_command(ping, &pinned, false, false).unwrap();
        // Values needing quoting stay a single argument.
        assert!(rendered.contains("'cache 1' PING"));
    }

    #[test]
    fn database_plugin_wraps_psql_with_docker_fallback() {
        let catalog = builtin_catalog().expect("built-in catalog should be valid");
        let database = catalog
            .iter()
            .find(|plugin| plugin.id == "database-inspector")
            .expect("Database plugin should be present");
        let PluginEntry::Commands { actions } = &database.entry else {
            panic!("Database plugin should expose command actions");
        };

        let databases = actions
            .iter()
            .find(|action| action.id == "postgres-databases")
            .expect("PostgreSQL databases action should be present");
        let rendered = render_command(databases, &BTreeMap::new(), false, false).unwrap();
        assert!(rendered.starts_with("sh -c "));
        assert!(rendered.contains("command -v psql"));
        assert!(rendered.contains("docker exec"));
        // container ($0) and docker user ($1) both default to empty arguments.
        assert!(rendered.contains("'' '' -Atqc"));
    }

    #[test]
    fn every_builtin_directory_is_registered_in_the_catalog() {
        let catalog_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("builtin");
        let mut dir_ids: HashSet<String> = std::fs::read_dir(&catalog_dir)
            .expect("builtin plugin directory should be readable")
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().map(|t| t.is_dir()).unwrap_or(false))
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        let registered: HashSet<String> = BUILTIN_MANIFESTS
            .iter()
            .map(|(id, _)| id.to_string())
            .collect();
        // Editor droppings must not sneak into the catalog directory.
        dir_ids.remove(".DS_Store");
        assert_eq!(
            dir_ids,
            registered,
            "every builtin/<id>/ directory must be registered in BUILTIN_MANIFESTS and vice versa"
        );
    }

    #[test]
    fn documented_external_plugin_example_is_valid() {
        let example = include_str!("../../examples/plugins/system-info/plugin.json");
        let manifest = parse_manifest(example, ManifestValidationPolicy::External)
            .expect("example manifest should be valid");
        assert_eq!(manifest.id, "example.system-info");
    }

    #[test]
    fn external_plugins_cannot_load_native_views() {
        let manifest = external_manifest(PluginEntry::Native {
            view: "server-status".to_string(),
        });
        assert_eq!(
            validate_manifest(&manifest, ManifestValidationPolicy::External).unwrap_err(),
            "External plugins cannot declare native views"
        );
    }

    #[test]
    fn external_plugins_cannot_request_host_permissions() {
        let mut manifest = external_manifest(command_entry());
        manifest.permissions.push(PluginPermission::LocalSystemRead);
        assert_eq!(
            validate_manifest(&manifest, ManifestValidationPolicy::External).unwrap_err(),
            "External plugins may only request remote_exec or local_exec permission"
        );
    }

    #[test]
    fn command_plugins_claiming_local_session_support_require_local_exec_permission() {
        // The old "currently support SSH sessions only" restriction is gone:
        // local session support is allowed but demands the local_exec permission.
        let mut manifest = external_manifest(command_entry());
        manifest.session_types = vec![PluginSessionType::Local];
        assert_eq!(
            validate_manifest(&manifest, ManifestValidationPolicy::External).unwrap_err(),
            "Command plugins supporting local sessions must request local_exec permission"
        );
    }

    #[test]
    fn manifest_rejects_oversized_default_settings() {
        let mut manifest = external_manifest(command_entry());
        manifest.default_settings = serde_json::json!({
            "value": "x".repeat(MAX_PLUGIN_SETTINGS_BYTES)
        });
        assert!(
            validate_manifest(&manifest, ManifestValidationPolicy::External)
                .unwrap_err()
                .contains("defaultSettings exceeds")
        );
    }

    #[test]
    fn manifest_caps_rendered_inputs_per_action() {
        let mut entry = command_entry();
        let PluginEntry::Commands { actions } = &mut entry else {
            unreachable!();
        };
        actions[0].inputs = (0..9)
            .map(|index| PluginInput {
                id: format!("input{}", index),
                label: format!("Input {}", index),
                description: String::new(),
                placeholder: String::new(),
                required: false,
                kind: PluginInputKind::Text,
                options: Vec::new(),
            })
            .collect();
        let manifest = external_manifest(entry);

        assert!(
            validate_manifest(&manifest, ManifestValidationPolicy::External)
                .unwrap_err()
                .contains("too many inputs")
        );
    }

    #[test]
    fn manifest_caps_select_options() {
        let mut entry = command_entry();
        let PluginEntry::Commands { actions } = &mut entry else {
            unreachable!();
        };
        actions[0].inputs = vec![PluginInput {
            id: "target".to_string(),
            label: "Target".to_string(),
            description: String::new(),
            placeholder: String::new(),
            required: false,
            kind: PluginInputKind::Select,
            options: (0..33).map(|index| format!("option-{}", index)).collect(),
        }];
        let manifest = external_manifest(entry);

        assert!(
            validate_manifest(&manifest, ManifestValidationPolicy::External)
                .unwrap_err()
                .contains("too many options")
        );
    }

    #[test]
    fn command_arguments_quote_user_input_as_one_shell_argument() {
        let action = PluginAction {
            id: "show-logs".to_string(),
            name: "Show logs".to_string(),
            description: "Read logs".to_string(),
            program: "docker".to_string(),
            args: vec!["logs".to_string(), "{{input.container}}".to_string()],
            inputs: vec![PluginInput {
                id: "container".to_string(),
                label: "Container".to_string(),
                description: String::new(),
                placeholder: String::new(),
                required: true,
                kind: PluginInputKind::Text,
                options: Vec::new(),
            }],
            requires_confirmation: false,
            elevate: false,
            allow_sudo: false,
            output: PluginOutput::default(),
        };
        let inputs = BTreeMap::from([(
            "container".to_string(),
            Value::String("api; rm -rf /".to_string()),
        )]);

        assert_eq!(
            render_remote_command(&action, &inputs).unwrap(),
            "docker logs 'api; rm -rf /'"
        );
    }

    #[test]
    fn input_values_are_not_reexpanded_as_other_templates() {
        let action = PluginAction {
            id: "show-values".to_string(),
            name: "Show values".to_string(),
            description: "Show two literal values".to_string(),
            program: "tool".to_string(),
            args: vec![
                "{{input.first}}".to_string(),
                "{{input.second}}".to_string(),
            ],
            inputs: vec![
                PluginInput {
                    id: "first".to_string(),
                    label: "First".to_string(),
                    description: String::new(),
                    placeholder: String::new(),
                    required: true,
                    kind: PluginInputKind::Text,
                    options: Vec::new(),
                },
                PluginInput {
                    id: "second".to_string(),
                    label: "Second".to_string(),
                    description: String::new(),
                    placeholder: String::new(),
                    required: true,
                    kind: PluginInputKind::Text,
                    options: Vec::new(),
                },
            ],
            requires_confirmation: false,
            elevate: false,
            allow_sudo: false,
            output: PluginOutput::default(),
        };
        let inputs = BTreeMap::from([
            (
                "first".to_string(),
                Value::String("{{input.second}}".to_string()),
            ),
            ("second".to_string(), Value::String("literal".to_string())),
        ]);

        assert_eq!(
            render_remote_command(&action, &inputs).unwrap(),
            "tool '{{input.second}}' literal"
        );
    }

    #[test]
    fn manifest_rejects_undeclared_input_templates() {
        let action = PluginAction {
            id: "inspect".to_string(),
            name: "Inspect".to_string(),
            description: "Inspect a target".to_string(),
            program: "tool".to_string(),
            args: vec!["{{input.target}}".to_string()],
            inputs: Vec::new(),
            requires_confirmation: false,
            elevate: false,
            allow_sudo: false,
            output: PluginOutput::default(),
        };
        let manifest = external_manifest(PluginEntry::Commands {
            actions: vec![action],
        });

        assert!(
            validate_manifest(&manifest, ManifestValidationPolicy::External)
                .unwrap_err()
                .contains("undeclared input target")
        );
    }

    #[test]
    fn manifest_rejects_inputs_embedded_inside_arguments() {
        let action = PluginAction {
            id: "script".to_string(),
            name: "Script".to_string(),
            description: "Run a shell script".to_string(),
            program: "printf".to_string(),
            args: vec!["prefix={{input.value}}".to_string()],
            inputs: vec![PluginInput {
                id: "value".to_string(),
                label: "Value".to_string(),
                description: String::new(),
                placeholder: String::new(),
                required: true,
                kind: PluginInputKind::Text,
                options: Vec::new(),
            }],
            requires_confirmation: false,
            elevate: false,
            allow_sudo: false,
            output: PluginOutput::default(),
        };
        let manifest = external_manifest(PluginEntry::Commands {
            actions: vec![action],
        });

        assert!(
            validate_manifest(&manifest, ManifestValidationPolicy::External)
                .unwrap_err()
                .contains("input templates must occupy a complete argument")
        );
    }

    #[test]
    fn elevated_action_requires_explicit_confirmation() {
        let mut entry = command_entry();
        let PluginEntry::Commands { actions } = &mut entry else {
            unreachable!();
        };
        actions[0].elevate = true;
        actions[0].requires_confirmation = false;
        let manifest = external_manifest(entry);

        assert_eq!(
            validate_manifest(&manifest, ManifestValidationPolicy::External).unwrap_err(),
            "Action version requests elevation and must set requiresConfirmation"
        );
    }

    #[test]
    fn elevated_action_renders_sudo_stdin_flag_when_password_supplied() {
        let mut action = PluginAction {
            id: "restart".to_string(),
            name: "Restart".to_string(),
            description: "Restart a docker container".to_string(),
            program: "docker".to_string(),
            args: vec!["restart".to_string(), "{{input.container}}".to_string()],
            inputs: vec![PluginInput {
                id: "container".to_string(),
                label: "Container".to_string(),
                description: String::new(),
                placeholder: String::new(),
                required: true,
                kind: PluginInputKind::Text,
                options: Vec::new(),
            }],
            requires_confirmation: true,
            elevate: true,
            allow_sudo: false,
            output: PluginOutput::default(),
        };
        let inputs = BTreeMap::from([("container".to_string(), Value::String("web".to_string()))]);

        // With a password → `sudo -S -p ''` so we can feed stdin.
        assert_eq!(
            render_command(&action, &inputs, true, true).unwrap(),
            "sudo -S -p '' docker restart web"
        );

        // Without a password → `sudo -n` for NOPASSWD hosts.
        assert_eq!(
            render_command(&action, &inputs, true, false).unwrap(),
            "sudo -n docker restart web"
        );

        // Non-elevated actions ignore the password flag.
        action.elevate = false;
        assert_eq!(
            render_command(&action, &inputs, false, true).unwrap(),
            "docker restart web"
        );

        // Re-enable elevation for the injection-safety case below.
        action.elevate = true;

        // Input values with shell metacharacters stay quoted even after the
        // sudo prefix is added, so the existing injection-safety guarantees
        // are preserved for elevated actions.
        let dangerous = BTreeMap::from([(
            "container".to_string(),
            Value::String("web; rm -rf /".to_string()),
        )]);
        assert_eq!(
            render_command(&action, &dangerous, true, true).unwrap(),
            "sudo -S -p '' docker restart 'web; rm -rf /'"
        );
    }

    #[test]
    fn legacy_render_remote_command_defaults_to_sudo_n_when_elevated() {
        let action = PluginAction {
            id: "rollback".to_string(),
            name: "Rollback".to_string(),
            description: "Kernel rollback".to_string(),
            program: "reboot".to_string(),
            args: Vec::new(),
            inputs: Vec::new(),
            requires_confirmation: true,
            elevate: true,
            allow_sudo: false,
            output: PluginOutput::default(),
        };
        let inputs = BTreeMap::new();

        assert_eq!(
            render_remote_command(&action, &inputs).unwrap(),
            "sudo -n reboot"
        );
    }

    #[test]
    fn command_plugin_with_local_session_type_must_request_local_exec_permission() {
        let mut manifest = external_manifest(command_entry());
        manifest.session_types = vec![PluginSessionType::Ssh, PluginSessionType::Local];
        // built-in helpers only grant RemoteExec; this should now require
        // LocalExec as well.
        assert_eq!(
            validate_manifest(&manifest, ManifestValidationPolicy::Builtin).unwrap_err(),
            "Command plugins supporting local sessions must request local_exec permission"
        );

        manifest.permissions.push(PluginPermission::LocalExec);
        assert!(validate_manifest(&manifest, ManifestValidationPolicy::Builtin).is_ok());
    }

    #[test]
    fn external_plugin_can_request_local_exec_permission() {
        let mut manifest = external_manifest(command_entry());
        manifest.session_types = vec![PluginSessionType::Local];
        manifest.permissions = vec![PluginPermission::LocalExec];
        assert!(validate_manifest(&manifest, ManifestValidationPolicy::External).is_ok());
    }
}
