use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use glob::{glob, Pattern};

use super::{
    expand_config_path, home_dir, local_username, parse_bool, ImportCandidate, ImportSourceKind,
};
use crate::storage::AuthType;

#[derive(Debug)]
struct SshBlock {
    patterns: Vec<String>,
    directives: Vec<(String, String)>,
}

pub(super) fn default_path() -> Option<PathBuf> {
    home_dir().map(|home| home.join(".ssh").join("config"))
}

pub(super) fn parse(path: &Path, warnings: &mut Vec<String>) -> Result<Vec<ImportCandidate>> {
    let mut visited = HashSet::new();
    let lines = expand_file(path, &mut visited, warnings, 0)?;
    let mut blocks = vec![SshBlock {
        patterns: vec!["*".to_string()],
        directives: Vec::new(),
    }];
    let mut current = 0usize;
    let mut saw_match = false;

    for line in lines {
        let Some((key, value)) = parse_directive(&line) else {
            continue;
        };
        match key.as_str() {
            "host" => {
                blocks.push(SshBlock {
                    patterns: split_words(&value),
                    directives: Vec::new(),
                });
                current = blocks.len() - 1;
            }
            "match" => {
                saw_match = true;
                blocks.push(SshBlock {
                    patterns: Vec::new(),
                    directives: Vec::new(),
                });
                current = blocks.len() - 1;
            }
            _ => blocks[current].directives.push((key, value)),
        }
    }

    if saw_match {
        warnings.push(format!(
            "{} contains Match sections; conditional Match-only directives were skipped",
            path.display()
        ));
    }

    let mut aliases = Vec::new();
    let mut seen_aliases = HashSet::new();
    for block in &blocks {
        for pattern in &block.patterns {
            let alias = pattern.trim_start_matches('!');
            if pattern.starts_with('!') || contains_wildcard(alias) || alias.is_empty() {
                continue;
            }
            if seen_aliases.insert(alias.to_ascii_lowercase()) {
                aliases.push(alias.to_string());
            }
        }
    }

    let home = home_dir().unwrap_or_default();
    let default_user = local_username();
    let mut candidates = Vec::new();
    for alias in aliases {
        let mut hostname = None;
        let mut username = None;
        let mut port = None;
        let mut identity_file = None;
        let mut proxy_jump = None;
        let mut remote_command = None;
        let mut forward_agent = None;

        // OpenSSH uses the first obtained value for each parameter. Evaluating
        // matching blocks in source order preserves that behavior.
        for block in &blocks {
            if !patterns_match(&block.patterns, &alias) {
                continue;
            }
            for (key, value) in &block.directives {
                match key.as_str() {
                    "hostname" if hostname.is_none() => hostname = Some(value.clone()),
                    "user" if username.is_none() => username = Some(value.clone()),
                    "port" if port.is_none() => port = value.parse::<u16>().ok(),
                    "identityfile" if identity_file.is_none() => {
                        identity_file = Some(value.clone())
                    }
                    "proxyjump" if proxy_jump.is_none() => proxy_jump = Some(value.clone()),
                    "remotecommand" if remote_command.is_none() => {
                        remote_command = Some(value.clone())
                    }
                    "forwardagent" if forward_agent.is_none() => {
                        forward_agent = Some(parse_bool(value))
                    }
                    _ => {}
                }
            }
        }

        let username = username.unwrap_or_else(|| default_user.clone());
        let port = port.unwrap_or(22);
        let hostname = hostname
            .unwrap_or_else(|| alias.clone())
            .replace("%h", &alias);
        if hostname.is_empty() {
            continue;
        }
        let key_path = identity_file
            .filter(|value| !value.eq_ignore_ascii_case("none"))
            .map(|value| {
                expand_identity_path(&value, &home, &hostname, &username, port)
                    .to_string_lossy()
                    .into_owned()
            });
        if let Some(key_path) = key_path.as_deref() {
            if !Path::new(key_path).is_file() {
                warnings.push(format!(
                    "OpenSSH profile '{}' references a private key that is not currently readable: {}",
                    alias, key_path
                ));
            }
        }

        candidates.push(ImportCandidate {
            source: ImportSourceKind::OpenSsh,
            source_name: alias.clone(),
            name: alias,
            host: hostname,
            port,
            username,
            auth_type: if key_path.is_some() {
                AuthType::KeyWithPassphrase
            } else {
                AuthType::Password
            },
            key_path,
            jump_host: proxy_jump.and_then(|value| parse_proxy_jump(&value)),
            post_login_command: remote_command.filter(|value| !value.eq_ignore_ascii_case("none")),
            agent_forwarding: forward_agent.unwrap_or(false),
            tags: vec!["import:openssh".to_string()],
        });
    }

    Ok(candidates)
}

fn expand_file(
    path: &Path,
    visited: &mut HashSet<PathBuf>,
    warnings: &mut Vec<String>,
    depth: usize,
) -> Result<Vec<String>> {
    if depth > 16 {
        bail!(
            "OpenSSH Include nesting exceeded 16 levels at {}",
            path.display()
        );
    }
    let identity = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if !visited.insert(identity.clone()) {
        warnings.push(format!(
            "Skipped cyclic OpenSSH Include: {}",
            path.display()
        ));
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read OpenSSH config {}", path.display()))?;
    let mut expanded = Vec::new();
    for raw_line in content.lines() {
        let line = strip_unquoted_comment(raw_line).trim().to_string();
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = parse_directive(&line) else {
            continue;
        };
        if key != "include" {
            expanded.push(line);
            continue;
        }

        for include in split_words(&value) {
            let include_pattern = expand_config_path(&include, path.parent());
            let pattern_string = include_pattern.to_string_lossy().into_owned();
            let mut matches: Vec<PathBuf> = if contains_wildcard(&pattern_string) {
                glob(&pattern_string)
                    .with_context(|| format!("Invalid OpenSSH Include pattern {pattern_string}"))?
                    .filter_map(std::result::Result::ok)
                    .collect()
            } else if include_pattern.is_file() {
                vec![include_pattern]
            } else {
                Vec::new()
            };
            matches.sort();
            for included in matches {
                expanded.extend(expand_file(&included, visited, warnings, depth + 1)?);
            }
        }
    }
    visited.remove(&identity);
    Ok(expanded)
}

fn parse_directive(line: &str) -> Option<(String, String)> {
    let words = split_words(line);
    let first = words.first()?;
    if let Some((key, first_value)) = first.split_once('=') {
        let mut values = Vec::new();
        if !first_value.is_empty() {
            values.push(first_value.to_string());
        }
        values.extend(words.iter().skip(1).cloned());
        return Some((key.to_ascii_lowercase(), values.join(" ")));
    }
    Some((
        first.to_ascii_lowercase(),
        words.iter().skip(1).cloned().collect::<Vec<_>>().join(" "),
    ))
}

fn split_words(value: &str) -> Vec<String> {
    shlex::split(value).unwrap_or_else(|| value.split_whitespace().map(str::to_string).collect())
}

fn strip_unquoted_comment(line: &str) -> String {
    let mut result = String::new();
    let mut quote = None;
    let mut escaped = false;
    for character in line.chars() {
        if escaped {
            result.push(character);
            escaped = false;
            continue;
        }
        if character == '\\' {
            result.push(character);
            escaped = true;
            continue;
        }
        if matches!(character, '\'' | '"') {
            if quote == Some(character) {
                quote = None;
            } else if quote.is_none() {
                quote = Some(character);
            }
            result.push(character);
            continue;
        }
        if character == '#' && quote.is_none() {
            break;
        }
        result.push(character);
    }
    result
}

fn patterns_match(patterns: &[String], alias: &str) -> bool {
    if patterns.is_empty() {
        return false;
    }
    let alias = alias.to_ascii_lowercase();
    let mut positive = false;
    for raw in patterns {
        let (negated, pattern) = raw
            .strip_prefix('!')
            .map(|pattern| (true, pattern))
            .unwrap_or((false, raw.as_str()));
        let pattern = pattern.to_ascii_lowercase();
        let matched = Pattern::new(&pattern)
            .map(|pattern| pattern.matches(&alias))
            .unwrap_or_else(|_| pattern == alias);
        if negated && matched {
            return false;
        }
        if !negated && matched {
            positive = true;
        }
    }
    positive
}

fn contains_wildcard(value: &str) -> bool {
    value
        .chars()
        .any(|character| matches!(character, '*' | '?' | '['))
}

fn expand_identity_path(
    value: &str,
    home: &Path,
    host: &str,
    username: &str,
    port: u16,
) -> PathBuf {
    let value = value
        .replace("%d", &home.to_string_lossy())
        .replace("%h", host)
        .replace("%r", username)
        .replace("%p", &port.to_string());
    let ssh_dir = home.join(".ssh");
    expand_config_path(&value, Some(&ssh_dir))
}

fn parse_proxy_jump(value: &str) -> Option<String> {
    let first = value.split(',').next()?.trim();
    if first.is_empty() || first.eq_ignore_ascii_case("none") {
        return None;
    }
    let host = first
        .rsplit_once('@')
        .map(|(_, host)| host)
        .unwrap_or(first);
    if let Some(stripped) = host.strip_prefix('[') {
        return stripped.split(']').next().map(str::to_string);
    }
    if let Some((host, port)) = host.rsplit_once(':') {
        if port.parse::<u16>().is_ok() {
            return Some(host.to_string());
        }
    }
    Some(host.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn parses_hosts_includes_and_proxy_jump() {
        let temp = TempDir::new().unwrap();
        let ssh_dir = temp.path().join(".ssh");
        fs::create_dir_all(&ssh_dir).unwrap();
        fs::write(
            ssh_dir.join("extra.conf"),
            "Host bastion\n  HostName bastion.example.com\n  User jump\n",
        )
        .unwrap();
        fs::write(ssh_dir.join("id_ed25519"), "test key").unwrap();
        fs::write(
            ssh_dir.join("config"),
            format!(
                "Include {}\nHost prod\n  HostName 10.0.0.5\n  User deploy\n  IdentityFile {}\n  ProxyJump bastion\n  ForwardAgent yes\nHost *\n  Port 2222\n",
                ssh_dir.join("extra.conf").display(),
                ssh_dir.join("id_ed25519").display()
            ),
        )
        .unwrap();

        let mut warnings = Vec::new();
        let parsed = parse(&ssh_dir.join("config"), &mut warnings).unwrap();
        let prod = parsed
            .iter()
            .find(|profile| profile.name == "prod")
            .unwrap();
        assert_eq!(prod.host, "10.0.0.5");
        assert_eq!(prod.username, "deploy");
        assert_eq!(prod.port, 2222);
        assert_eq!(prod.jump_host.as_deref(), Some("bastion"));
        assert!(prod.agent_forwarding);
        assert_eq!(prod.auth_type, AuthType::KeyWithPassphrase);
        assert!(parsed.iter().any(|profile| profile.name == "bastion"));
    }

    #[test]
    fn respects_first_obtained_value() {
        let temp = TempDir::new().unwrap();
        let config = temp.path().join("config");
        fs::write(
            &config,
            "Host prod\n  User deploy\nHost *\n  User fallback\n  Port 2200\n",
        )
        .unwrap();
        let mut warnings = Vec::new();
        let parsed = parse(&config, &mut warnings).unwrap();
        assert_eq!(parsed[0].username, "deploy");
        assert_eq!(parsed[0].port, 2200);
    }
}
