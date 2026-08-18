use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use super::{
    expand_config_path, home_dir, local_username, parse_bool, ImportCandidate, ImportSourceKind,
};
use crate::storage::AuthType;

pub(super) fn default_path() -> Option<PathBuf> {
    if cfg!(windows) {
        None
    } else {
        home_dir().map(|home| home.join(".putty").join("sessions"))
    }
}

pub(super) fn source_available(path: Option<&Path>) -> bool {
    if path.map(Path::exists).unwrap_or(false) {
        return true;
    }
    #[cfg(windows)]
    {
        return registry_profiles()
            .map(|profiles| !profiles.is_empty())
            .unwrap_or(false);
    }
    #[cfg(not(windows))]
    {
        false
    }
}

pub(super) fn parse(
    path: Option<&Path>,
    warnings: &mut Vec<String>,
) -> Result<Vec<ImportCandidate>> {
    let profiles = if let Some(path) = path {
        if !path.exists() {
            bail!("PuTTY source does not exist: {}", path.display());
        }
        if path.is_dir() {
            parse_directory(path)?
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.eq_ignore_ascii_case("reg"))
            .unwrap_or(false)
        {
            parse_reg_file(path)?
        } else {
            let values = parse_session_text(&read_text_file(path)?);
            vec![(
                decode_session_name(
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("PuTTY session"),
                ),
                values,
            )]
        }
    } else {
        #[cfg(windows)]
        {
            registry_profiles()?
        }
        #[cfg(not(windows))]
        {
            Vec::new()
        }
    };

    let mut candidates = Vec::new();
    for (session_name, values) in profiles {
        if session_name.eq_ignore_ascii_case("Default Settings") {
            continue;
        }
        let protocol = putty_value(&values, "Protocol").unwrap_or("ssh");
        if !protocol.eq_ignore_ascii_case("ssh") {
            continue;
        }
        let Some(mut host) = putty_value(&values, "HostName").map(str::to_string) else {
            continue;
        };
        if host.trim().is_empty() {
            continue;
        }

        let mut username = putty_value(&values, "UserName")
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string);
        if username.is_none() {
            if let Some((user, hostname)) = host.split_once('@') {
                username = Some(user.to_string());
                host = hostname.to_string();
            }
        }
        let username = username.unwrap_or_else(local_username);
        let port = putty_value(&values, "PortNumber")
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(22);

        let key_path = putty_value(&values, "PublicKeyFile")
            .filter(|value| !value.trim().is_empty())
            .map(|value| {
                expand_config_path(value, home_dir().as_deref())
                    .to_string_lossy()
                    .into_owned()
            });
        let key_path = match key_path {
            Some(path)
                if Path::new(&path)
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .map(|extension| extension.eq_ignore_ascii_case("ppk"))
                    .unwrap_or(false) =>
            {
                warnings.push(format!(
                    "PuTTY profile '{}' uses a .ppk key; convert it to OpenSSH format or select another key in VibeShell",
                    session_name
                ));
                None
            }
            Some(path) => {
                if !Path::new(&path).is_file() {
                    warnings.push(format!(
                        "PuTTY profile '{}' references a private key that is not currently readable: {}",
                        session_name, path
                    ));
                }
                Some(path)
            }
            None => None,
        };

        if putty_value(&values, "ProxyHost")
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false)
        {
            warnings.push(format!(
                "PuTTY profile '{}' has proxy settings; non-SSH PuTTY proxy chains are not imported as jump hosts",
                session_name
            ));
        }

        candidates.push(ImportCandidate {
            source: ImportSourceKind::Putty,
            source_name: session_name.clone(),
            name: session_name,
            host,
            port,
            username,
            auth_type: if key_path.is_some() {
                AuthType::KeyWithPassphrase
            } else {
                AuthType::Password
            },
            key_path,
            jump_host: None,
            post_login_command: putty_value(&values, "RemoteCommand")
                .filter(|value| !value.trim().is_empty())
                .map(str::to_string),
            agent_forwarding: putty_value(&values, "AgentFwd")
                .map(parse_bool)
                .unwrap_or(false),
            tags: vec!["import:putty".to_string()],
        });
    }

    Ok(candidates)
}

fn parse_directory(path: &Path) -> Result<Vec<(String, BTreeMap<String, String>)>> {
    let mut entries: Vec<PathBuf> = fs::read_dir(path)
        .with_context(|| format!("Failed to read PuTTY sessions directory {}", path.display()))?
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect();
    entries.sort();

    entries
        .into_iter()
        .map(|path| {
            let name = decode_session_name(
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("PuTTY session"),
            );
            let values = parse_session_text(&read_text_file(&path)?);
            Ok((name, values))
        })
        .collect()
}

fn parse_reg_file(path: &Path) -> Result<Vec<(String, BTreeMap<String, String>)>> {
    let text = read_text_file(path)?;
    let marker = "\\putty\\sessions\\";
    let mut sessions = Vec::new();
    let mut current: Option<(String, BTreeMap<String, String>)> = None;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if let Some(session) = current.take() {
                sessions.push(session);
            }
            let lower = trimmed.to_ascii_lowercase();
            current = lower.find(marker).map(|index| {
                let start = index + marker.len();
                let encoded = &trimmed[start..trimmed.len() - 1];
                (decode_session_name(encoded), BTreeMap::new())
            });
            continue;
        }
        let Some((_, values)) = current.as_mut() else {
            continue;
        };
        if let Some((key, value)) = parse_reg_value(trimmed) {
            values.insert(key, value);
        }
    }
    if let Some(session) = current {
        sessions.push(session);
    }
    Ok(sessions)
}

fn parse_session_text(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|line| line.trim().split_once('='))
        .map(|(key, value)| (key.trim().to_string(), value.trim().to_string()))
        .collect()
}

fn parse_reg_value(line: &str) -> Option<(String, String)> {
    let (key, raw_value) = line.split_once('=')?;
    let key = key.trim().trim_matches('"').to_string();
    let raw_value = raw_value.trim();
    if let Some(hex) = raw_value.strip_prefix("dword:") {
        return u32::from_str_radix(hex, 16)
            .ok()
            .map(|value| (key, value.to_string()));
    }
    let value = raw_value.strip_prefix('"')?.strip_suffix('"')?;
    Some((key, unescape_reg_string(value)))
}

fn unescape_reg_string(value: &str) -> String {
    let mut result = String::new();
    let mut characters = value.chars();
    while let Some(character) = characters.next() {
        if character == '\\' {
            match characters.next() {
                Some('\\') => result.push('\\'),
                Some('"') => result.push('"'),
                Some('n') => result.push('\n'),
                Some('r') => result.push('\r'),
                Some(other) => {
                    result.push('\\');
                    result.push(other);
                }
                None => result.push('\\'),
            }
        } else {
            result.push(character);
        }
    }
    result
}

fn read_text_file(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("Failed to read {}", path.display()))?;
    if bytes.starts_with(&[0xff, 0xfe]) {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();
        return String::from_utf16(&units)
            .with_context(|| format!("Invalid UTF-16 text in {}", path.display()));
    }
    if bytes.starts_with(&[0xfe, 0xff]) {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|chunk| u16::from_be_bytes([chunk[0], chunk[1]]))
            .collect();
        return String::from_utf16(&units)
            .with_context(|| format!("Invalid UTF-16 text in {}", path.display()));
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn decode_session_name(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) =
                (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
            {
                decoded.push(high * 16 + low);
                index += 3;
                continue;
            }
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn putty_value<'a>(values: &'a BTreeMap<String, String>, key: &str) -> Option<&'a str> {
    values
        .iter()
        .find(|(candidate, _)| candidate.eq_ignore_ascii_case(key))
        .map(|(_, value)| value.as_str())
}

#[cfg(windows)]
fn registry_profiles() -> Result<Vec<(String, BTreeMap<String, String>)>> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};
    use winreg::RegKey;

    let current_user = RegKey::predef(HKEY_CURRENT_USER);
    let sessions = match current_user
        .open_subkey_with_flags("Software\\SimonTatham\\PuTTY\\Sessions", KEY_READ)
    {
        Ok(sessions) => sessions,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };

    let mut profiles = Vec::new();
    for encoded_name in sessions.enum_keys().filter_map(std::result::Result::ok) {
        let session = sessions.open_subkey_with_flags(&encoded_name, KEY_READ)?;
        let mut values = BTreeMap::new();
        for key in [
            "HostName",
            "PortNumber",
            "UserName",
            "PublicKeyFile",
            "Protocol",
            "RemoteCommand",
            "AgentFwd",
            "ProxyHost",
            "ProxyPort",
            "ProxyUsername",
            "ProxyMethod",
        ] {
            if let Ok(value) = session.get_value::<String, _>(key) {
                values.insert(key.to_string(), value);
            } else if let Ok(value) = session.get_value::<u32, _>(key) {
                values.insert(key.to_string(), value.to_string());
            }
        }
        profiles.push((decode_session_name(&encoded_name), values));
    }
    Ok(profiles)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn parses_registry_exports() {
        let temp = TempDir::new().unwrap();
        let export = temp.path().join("putty.reg");
        fs::write(
            &export,
            r#"Windows Registry Editor Version 5.00

[HKEY_CURRENT_USER\Software\SimonTatham\PuTTY\Sessions\Prod%20Web]
"HostName"="prod.example.com"
"PortNumber"=dword:0000089a
"UserName"="deploy"
"Protocol"="ssh"
"AgentFwd"=dword:00000001
"RemoteCommand"="cd /srv/app"
"#,
        )
        .unwrap();

        let mut warnings = Vec::new();
        let parsed = parse(Some(&export), &mut warnings).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, "Prod Web");
        assert_eq!(parsed[0].port, 2202);
        assert!(parsed[0].agent_forwarding);
        assert_eq!(parsed[0].post_login_command.as_deref(), Some("cd /srv/app"));
    }

    #[test]
    fn warns_and_imports_metadata_for_ppk_profiles() {
        let temp = TempDir::new().unwrap();
        let session = temp.path().join("PPK%20Server");
        fs::write(
            &session,
            "HostName=ppk.example.com\nUserName=deploy\nPublicKeyFile=/keys/id.ppk\n",
        )
        .unwrap();

        let mut warnings = Vec::new();
        let parsed = parse(Some(&session), &mut warnings).unwrap();
        assert_eq!(parsed[0].name, "PPK Server");
        assert!(parsed[0].key_path.is_none());
        assert!(warnings.iter().any(|warning| warning.contains(".ppk")));
    }
}
