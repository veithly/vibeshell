//! Import saved SSH profiles from OpenSSH, PuTTY, and Tabby.
//!
//! Parsers import connection metadata only. Passwords are never copied from
//! third-party tools, and private keys are referenced by path so VibeShell can
//! read them locally when a connection is established.

mod openssh;
mod putty;
mod tabby;

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::storage::{AuthType, Database, Server};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ImportSourceKind {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "openssh")]
    OpenSsh,
    #[serde(rename = "putty")]
    Putty,
    #[serde(rename = "tabby")]
    Tabby,
}

impl ImportSourceKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::OpenSsh => "OpenSSH",
            Self::Putty => "PuTTY",
            Self::Tabby => "Tabby",
        }
    }
}

impl fmt::Display for ImportSourceKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedImportSource {
    pub kind: ImportSourceKind,
    pub label: String,
    pub available: bool,
    pub path: Option<String>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportCandidate {
    pub source: ImportSourceKind,
    pub source_name: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: AuthType,
    pub key_path: Option<String>,
    pub jump_host: Option<String>,
    pub post_login_command: Option<String>,
    pub agent_forwarding: bool,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub sources: Vec<DetectedImportSource>,
    pub servers: Vec<ImportCandidate>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedServerResult {
    pub source: ImportSourceKind,
    pub source_name: String,
    pub name: String,
    pub server_id: Option<String>,
    pub status: String,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub discovered: usize,
    pub imported: usize,
    pub skipped: usize,
    pub renamed: usize,
    pub servers: Vec<ImportedServerResult>,
    pub warnings: Vec<String>,
}

pub fn detect_import_sources() -> Vec<DetectedImportSource> {
    let openssh_path = openssh::default_path();
    let putty_path = putty::default_path();
    let tabby_path = tabby::default_path();

    vec![
        DetectedImportSource {
            kind: ImportSourceKind::OpenSsh,
            label: "OpenSSH config".to_string(),
            available: openssh_path
                .as_ref()
                .map(|path| path.is_file())
                .unwrap_or(false),
            path: openssh_path.as_ref().map(|path| display_path(path)),
            detail: "User SSH config, including Host, Include, IdentityFile, and ProxyJump"
                .to_string(),
        },
        DetectedImportSource {
            kind: ImportSourceKind::Putty,
            label: "PuTTY sessions".to_string(),
            available: putty::source_available(putty_path.as_deref()),
            path: putty_path.as_ref().map(|path| display_path(path)),
            detail: if cfg!(windows) {
                "HKCU\\Software\\SimonTatham\\PuTTY\\Sessions or an exported .reg file".to_string()
            } else {
                "~/.putty/sessions or an exported .reg file".to_string()
            },
        },
        DetectedImportSource {
            kind: ImportSourceKind::Tabby,
            label: "Tabby profiles".to_string(),
            available: tabby_path
                .as_ref()
                .map(|path| path.is_file())
                .unwrap_or(false),
            path: tabby_path.as_ref().map(|path| display_path(path)),
            detail: "Tabby config.yaml SSH profiles".to_string(),
        },
    ]
}

pub fn preview_import(
    source: ImportSourceKind,
    explicit_path: Option<PathBuf>,
) -> Result<ImportPreview> {
    if source == ImportSourceKind::Auto && explicit_path.is_some() {
        bail!("--path must be used with openssh, putty, or tabby, not auto");
    }

    let sources = detect_import_sources();
    let mut servers = Vec::new();
    let mut warnings = Vec::new();

    match source {
        ImportSourceKind::Auto => {
            for detected in &sources {
                if !detected.available {
                    continue;
                }
                let path = detected.path.as_deref().map(PathBuf::from);
                append_source(detected.kind, path.as_deref(), &mut servers, &mut warnings)?;
            }
        }
        kind => {
            let path = explicit_path.or_else(|| default_path_for(kind));
            if path.is_none() && kind != ImportSourceKind::Putty {
                bail!(
                    "Could not determine the default {} configuration path",
                    kind
                );
            }
            append_source(kind, path.as_deref(), &mut servers, &mut warnings)?;
        }
    }

    let mut seen = HashSet::new();
    servers.retain(|candidate| {
        seen.insert((
            candidate.source,
            candidate.name.to_ascii_lowercase(),
            candidate.host.to_ascii_lowercase(),
            candidate.port,
            candidate.username.to_ascii_lowercase(),
        ))
    });
    servers.sort_by(|left, right| {
        left.source.label().cmp(right.source.label()).then_with(|| {
            left.name
                .to_ascii_lowercase()
                .cmp(&right.name.to_ascii_lowercase())
        })
    });

    if servers.is_empty() {
        warnings.push(format!("No importable {} SSH profiles were found", source));
    }

    Ok(ImportPreview {
        sources,
        servers,
        warnings,
    })
}

pub fn import_preview(database: &Database, preview: &ImportPreview) -> Result<ImportReport> {
    let existing = database.server_list(None, None)?;
    let mut used_names: HashSet<String> = existing
        .iter()
        .map(|server| server.name.to_ascii_lowercase())
        .collect();
    let mut names_to_id: HashMap<String, String> = existing
        .iter()
        .map(|server| (server.name.to_ascii_lowercase(), server.id.clone()))
        .collect();
    let existing_by_name: HashMap<String, Server> = existing
        .into_iter()
        .map(|server| (server.name.to_ascii_lowercase(), server))
        .collect();

    let mut alias_to_id: HashMap<(ImportSourceKind, String), String> = HashMap::new();
    let mut pending_jumps: Vec<(String, ImportSourceKind, String)> = Vec::new();
    let mut report = ImportReport {
        discovered: preview.servers.len(),
        imported: 0,
        skipped: 0,
        renamed: 0,
        servers: Vec::new(),
        warnings: preview.warnings.clone(),
    };

    for candidate in &preview.servers {
        let requested_key = candidate.name.to_ascii_lowercase();
        if let Some(existing_server) = existing_by_name.get(&requested_key) {
            if same_endpoint(existing_server, candidate) {
                report.skipped += 1;
                remember_aliases(&mut alias_to_id, candidate, &existing_server.id);
                report.servers.push(ImportedServerResult {
                    source: candidate.source,
                    source_name: candidate.source_name.clone(),
                    name: existing_server.name.clone(),
                    server_id: Some(existing_server.id.clone()),
                    status: "skipped".to_string(),
                    message: Some("An equivalent VibeShell server already exists".to_string()),
                });
                continue;
            }
        }

        let (final_name, renamed) =
            unique_server_name(&candidate.name, candidate.source, &mut used_names);
        if renamed {
            report.renamed += 1;
        }

        let mut server = Server {
            id: String::new(),
            name: final_name,
            host: candidate.host.clone(),
            port: candidate.port,
            username: candidate.username.clone(),
            auth_type: candidate.auth_type.clone(),
            credential_id: None,
            group_id: None,
            tags: candidate.tags.clone(),
            created_at: 0,
            updated_at: 0,
            jump_host_id: None,
            post_login_command: candidate.post_login_command.clone(),
            agent_forwarding: candidate.agent_forwarding,
        };
        database.server_add(&mut server).with_context(|| {
            format!(
                "Failed to import {} profile '{}'",
                candidate.source, candidate.source_name
            )
        })?;

        if let Some(key_path) = candidate.key_path.as_deref() {
            match database.credential_save(
                &server.name,
                "key_with_passphrase",
                "",
                None,
                Some(key_path),
            ) {
                Ok(credential_id) => {
                    server.credential_id = Some(credential_id);
                    database.server_update(&server)?;
                }
                Err(error) => report.warnings.push(format!(
                    "Imported '{}' but could not save its private-key path: {}",
                    server.name, error
                )),
            }
        }

        names_to_id.insert(server.name.to_ascii_lowercase(), server.id.clone());
        remember_aliases(&mut alias_to_id, candidate, &server.id);
        if let Some(jump_host) = candidate.jump_host.as_deref() {
            pending_jumps.push((server.id.clone(), candidate.source, jump_host.to_string()));
        }

        report.imported += 1;
        report.servers.push(ImportedServerResult {
            source: candidate.source,
            source_name: candidate.source_name.clone(),
            name: server.name.clone(),
            server_id: Some(server.id.clone()),
            status: if renamed {
                "imported_renamed".to_string()
            } else {
                "imported".to_string()
            },
            message: renamed.then(|| {
                format!(
                    "Renamed because '{}' already existed in VibeShell",
                    candidate.name
                )
            }),
        });
    }

    for (server_id, source, jump_name) in pending_jumps {
        let jump_key = jump_name.to_ascii_lowercase();
        let jump_id = alias_to_id
            .get(&(source, jump_key.clone()))
            .or_else(|| names_to_id.get(&jump_key))
            .cloned();
        match jump_id {
            Some(jump_id) if jump_id != server_id => {
                if let Some(mut server) = database.server_get(&server_id)? {
                    server.jump_host_id = Some(jump_id);
                    database.server_update(&server)?;
                }
            }
            Some(_) => report.warnings.push(format!(
                "Ignored self-referencing jump host '{}' for server {}",
                jump_name, server_id
            )),
            None => report.warnings.push(format!(
                "Could not resolve jump host '{}' for imported server {}",
                jump_name, server_id
            )),
        }
    }

    Ok(report)
}

fn append_source(
    kind: ImportSourceKind,
    path: Option<&Path>,
    servers: &mut Vec<ImportCandidate>,
    warnings: &mut Vec<String>,
) -> Result<()> {
    let mut imported = match kind {
        ImportSourceKind::Auto => unreachable!("auto is expanded before parsing"),
        ImportSourceKind::OpenSsh => openssh::parse(
            path.context("OpenSSH config path is unavailable")?,
            warnings,
        )?,
        ImportSourceKind::Putty => putty::parse(path, warnings)?,
        ImportSourceKind::Tabby => {
            tabby::parse(path.context("Tabby config path is unavailable")?, warnings)?
        }
    };
    servers.append(&mut imported);
    Ok(())
}

fn default_path_for(kind: ImportSourceKind) -> Option<PathBuf> {
    match kind {
        ImportSourceKind::Auto => None,
        ImportSourceKind::OpenSsh => openssh::default_path(),
        ImportSourceKind::Putty => putty::default_path(),
        ImportSourceKind::Tabby => tabby::default_path(),
    }
}

fn remember_aliases(
    aliases: &mut HashMap<(ImportSourceKind, String), String>,
    candidate: &ImportCandidate,
    server_id: &str,
) {
    aliases.insert(
        (candidate.source, candidate.source_name.to_ascii_lowercase()),
        server_id.to_string(),
    );
    aliases.insert(
        (candidate.source, candidate.name.to_ascii_lowercase()),
        server_id.to_string(),
    );
}

fn same_endpoint(server: &Server, candidate: &ImportCandidate) -> bool {
    server.host.eq_ignore_ascii_case(&candidate.host)
        && server.port == candidate.port
        && server.username.eq_ignore_ascii_case(&candidate.username)
}

fn unique_server_name(
    requested: &str,
    source: ImportSourceKind,
    used_names: &mut HashSet<String>,
) -> (String, bool) {
    let requested = requested.trim();
    let base = if requested.is_empty() {
        format!("Imported {} server", source.label())
    } else {
        requested.to_string()
    };
    if used_names.insert(base.to_ascii_lowercase()) {
        return (base, false);
    }

    let source_base = format!("{} ({})", base, source.label());
    if used_names.insert(source_base.to_ascii_lowercase()) {
        return (source_base, true);
    }
    for suffix in 2.. {
        let candidate = format!("{} ({}) {}", base, source.label(), suffix);
        if used_names.insert(candidate.to_ascii_lowercase()) {
            return (candidate, true);
        }
    }
    unreachable!()
}

pub(super) fn home_dir() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf())
}

pub(super) fn local_username() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "user".to_string())
}

pub(super) fn parse_bool(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "yes" | "true" | "on"
    )
}

pub(super) fn expand_config_path(value: &str, relative_to: Option<&Path>) -> PathBuf {
    let expanded = expand_environment(value);
    if let Some(rest) = expanded.strip_prefix("~/") {
        return home_dir().unwrap_or_default().join(rest);
    }
    let path = PathBuf::from(expanded);
    if path.is_absolute() {
        path
    } else {
        relative_to.map(|parent| parent.join(&path)).unwrap_or(path)
    }
}

fn expand_environment(value: &str) -> String {
    let mut output = value.to_string();
    for (key, environment_value) in std::env::vars() {
        output = output.replace(&format!("${{{key}}}"), &environment_value);
        output = output.replace(&format!("%{key}%"), &environment_value);
    }
    output
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn imports_profiles_with_conflict_renaming_key_paths_and_jump_links() {
        let temp = TempDir::new().unwrap();
        let database = Database::new_at(temp.path().join("vibeshell.db")).unwrap();
        let mut existing = Server {
            id: String::new(),
            name: "App".to_string(),
            host: "old.example.com".to_string(),
            port: 22,
            username: "old".to_string(),
            auth_type: AuthType::Password,
            credential_id: None,
            group_id: None,
            tags: Vec::new(),
            created_at: 0,
            updated_at: 0,
            jump_host_id: None,
            post_login_command: None,
            agent_forwarding: false,
        };
        database.server_add(&mut existing).unwrap();

        let preview = ImportPreview {
            sources: Vec::new(),
            warnings: Vec::new(),
            servers: vec![
                ImportCandidate {
                    source: ImportSourceKind::OpenSsh,
                    source_name: "jump".to_string(),
                    name: "Jump".to_string(),
                    host: "jump.example.com".to_string(),
                    port: 22,
                    username: "jump".to_string(),
                    auth_type: AuthType::Password,
                    key_path: None,
                    jump_host: None,
                    post_login_command: None,
                    agent_forwarding: false,
                    tags: vec!["import:openssh".to_string()],
                },
                ImportCandidate {
                    source: ImportSourceKind::OpenSsh,
                    source_name: "app".to_string(),
                    name: "App".to_string(),
                    host: "app.example.com".to_string(),
                    port: 22,
                    username: "deploy".to_string(),
                    auth_type: AuthType::KeyWithPassphrase,
                    key_path: Some("/keys/app".to_string()),
                    jump_host: Some("jump".to_string()),
                    post_login_command: None,
                    agent_forwarding: false,
                    tags: vec!["import:openssh".to_string()],
                },
            ],
        };

        let report = import_preview(&database, &preview).unwrap();
        assert_eq!(report.imported, 2);
        assert_eq!(report.renamed, 1);
        let app = database
            .server_get_by_name("App (OpenSSH)")
            .unwrap()
            .unwrap();
        let jump = database.server_get_by_name("Jump").unwrap().unwrap();
        assert_eq!(app.jump_host_id.as_deref(), Some(jump.id.as_str()));
        let credential = database.credential_get(&app.name).unwrap().unwrap();
        assert_eq!(credential.credential, "");
        assert_eq!(credential.key_path.as_deref(), Some("/keys/app"));
    }
}
