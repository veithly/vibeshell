use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde_yaml::Value;

use super::{
    expand_config_path, home_dir, local_username, parse_bool, ImportCandidate, ImportSourceKind,
};
use crate::storage::AuthType;

pub(super) fn default_path() -> Option<PathBuf> {
    let home = home_dir()?;
    let mut candidates = Vec::new();

    if cfg!(target_os = "macos") {
        candidates.push(
            home.join("Library")
                .join("Application Support")
                .join("tabby")
                .join("config.yaml"),
        );
    }
    if cfg!(windows) {
        if let Some(app_data) = std::env::var_os("APPDATA") {
            let app_data = PathBuf::from(app_data);
            candidates.push(app_data.join("Tabby").join("config.yaml"));
            candidates.push(app_data.join("tabby").join("config.yaml"));
        }
    }
    if let Some(xdg_config) = std::env::var_os("XDG_CONFIG_HOME") {
        candidates.push(PathBuf::from(xdg_config).join("tabby").join("config.yaml"));
    }
    candidates.push(home.join(".config").join("tabby").join("config.yaml"));

    candidates
        .iter()
        .find(|path| path.is_file())
        .cloned()
        .or_else(|| candidates.into_iter().next())
}

pub(super) fn parse(path: &Path, warnings: &mut Vec<String>) -> Result<Vec<ImportCandidate>> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read Tabby config {}", path.display()))?;
    let root: Value = serde_yaml::from_str(&content)
        .with_context(|| format!("Failed to parse Tabby YAML {}", path.display()))?;
    let profiles = yaml_get(&root, "profiles")
        .and_then(Value::as_sequence)
        .ok_or_else(|| anyhow!("Tabby config {} has no profiles list", path.display()))?;

    let mut id_to_name = HashMap::new();
    for profile in profiles {
        let Some(options) = yaml_get(profile, "options") else {
            continue;
        };
        let Some(host) = yaml_string(options, "host") else {
            continue;
        };
        let name = yaml_string(profile, "name").unwrap_or(host);
        if let Some(id) = yaml_string(profile, "id") {
            id_to_name.insert(id, name);
        }
    }

    let mut candidates = Vec::new();
    for profile in profiles {
        let profile_type = yaml_string(profile, "type").unwrap_or_default();
        let Some(options) = yaml_get(profile, "options") else {
            continue;
        };
        let Some(host) = yaml_string(options, "host") else {
            continue;
        };
        if !profile_type.is_empty() && !profile_type.eq_ignore_ascii_case("ssh") {
            continue;
        }

        let source_name = yaml_string(profile, "id")
            .or_else(|| yaml_string(profile, "name"))
            .unwrap_or_else(|| host.clone());
        let name = yaml_string(profile, "name").unwrap_or_else(|| source_name.clone());
        let username = yaml_string(options, "user").unwrap_or_else(local_username);
        let port = yaml_u16(options, "port").unwrap_or(22);

        let mut key_path = yaml_string_list(options, "privateKeys").into_iter().next();
        if key_path.is_none() {
            key_path = yaml_string(options, "privateKey");
        }
        let key_path = key_path.map(|key_path| {
            expand_config_path(&key_path, path.parent())
                .to_string_lossy()
                .into_owned()
        });
        if let Some(key_path) = key_path.as_deref() {
            if !Path::new(key_path).is_file() {
                warnings.push(format!(
                    "Tabby profile '{}' references a private key that is not currently readable: {}",
                    name, key_path
                ));
            }
        }

        if yaml_string(options, "password")
            .map(|password| !password.is_empty())
            .unwrap_or(false)
        {
            warnings.push(format!(
                "Tabby profile '{}' contains a stored password; passwords are intentionally not imported",
                name
            ));
        }
        if yaml_string(options, "auth")
            .map(|auth| auth.eq_ignore_ascii_case("agent"))
            .unwrap_or(false)
            && key_path.is_none()
        {
            warnings.push(format!(
                "Tabby profile '{}' uses SSH agent authentication; add a key or credential in VibeShell before connecting",
                name
            ));
        }

        let jump_host = yaml_string(options, "jumpHost")
            .and_then(|jump| id_to_name.get(&jump).cloned().or(Some(jump)));
        let mut tags = vec!["import:tabby".to_string()];
        if let Some(group) = yaml_string(profile, "group") {
            if !group.trim().is_empty() {
                tags.push(format!("tabby-group:{group}"));
            }
        }

        candidates.push(ImportCandidate {
            source: ImportSourceKind::Tabby,
            source_name,
            name,
            host,
            port,
            username,
            auth_type: if key_path.is_some() {
                AuthType::KeyWithPassphrase
            } else {
                AuthType::Password
            },
            key_path,
            jump_host,
            post_login_command: yaml_scripts(options).into_iter().next(),
            agent_forwarding: yaml_bool(options, "agentForward").unwrap_or(false),
            tags,
        });
    }

    Ok(candidates)
}

fn yaml_get<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    value.as_mapping()?.get(Value::String(key.to_string()))
}

fn yaml_string(value: &Value, key: &str) -> Option<String> {
    match yaml_get(value, key)? {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn yaml_u16(value: &Value, key: &str) -> Option<u16> {
    yaml_get(value, key)
        .and_then(Value::as_u64)
        .and_then(|value| u16::try_from(value).ok())
        .or_else(|| yaml_string(value, key)?.parse().ok())
}

fn yaml_bool(value: &Value, key: &str) -> Option<bool> {
    yaml_get(value, key)
        .and_then(Value::as_bool)
        .or_else(|| yaml_string(value, key).map(|value| parse_bool(&value)))
}

fn yaml_string_list(value: &Value, key: &str) -> Vec<String> {
    yaml_get(value, key)
        .and_then(Value::as_sequence)
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn yaml_scripts(options: &Value) -> Vec<String> {
    yaml_get(options, "scripts")
        .and_then(Value::as_sequence)
        .map(|scripts| {
            scripts
                .iter()
                .filter_map(|script| match script {
                    Value::String(command) => Some(command.clone()),
                    Value::Mapping(_) => ["script", "command", "text"]
                        .into_iter()
                        .find_map(|key| yaml_string(script, key)),
                    _ => None,
                })
                .filter(|command| !command.trim().is_empty())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn parses_ssh_profiles_without_copying_passwords() {
        let temp = TempDir::new().unwrap();
        let config = temp.path().join("config.yaml");
        fs::write(
            &config,
            r#"profiles:
  - id: jump-id
    type: ssh
    name: Jump
    options:
      host: jump.example.com
      user: jump
  - id: app-id
    type: ssh
    name: App
    group: production
    options:
      host: app.example.com
      port: 2202
      user: deploy
      auth: password
      password: encrypted-value
      jumpHost: jump-id
      agentForward: true
      scripts:
        - command: cd /srv/app
"#,
        )
        .unwrap();

        let mut warnings = Vec::new();
        let parsed = parse(&config, &mut warnings).unwrap();
        let app = parsed.iter().find(|profile| profile.name == "App").unwrap();
        assert_eq!(app.port, 2202);
        assert_eq!(app.jump_host.as_deref(), Some("Jump"));
        assert_eq!(app.post_login_command.as_deref(), Some("cd /srv/app"));
        assert!(app.agent_forwarding);
        assert!(app.tags.contains(&"tabby-group:production".to_string()));
        assert!(warnings.iter().any(|warning| warning.contains("password")));
    }
}
