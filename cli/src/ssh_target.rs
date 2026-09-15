//! Parse `user@host[:port]` shorthand used by `vibeshell servers add`.

use anyhow::{bail, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshTarget {
    pub username: Option<String>,
    pub host: String,
    pub port: Option<u16>,
}

pub fn parse_ssh_target(input: &str) -> Result<SshTarget> {
    let input = input.trim();
    if input.is_empty() {
        bail!("Target is required (user@host[:port])");
    }

    let (username, hostport) = match input.split_once('@') {
        Some((user, rest)) => {
            if user.is_empty() {
                bail!("Username is empty in '{input}'");
            }
            if rest.is_empty() {
                bail!("Host is empty in '{input}'");
            }
            (Some(user.to_string()), rest)
        }
        None => (None, input),
    };

    let (host, port) = split_host_port(hostport)?;
    if host.is_empty() {
        bail!("Host is empty in '{input}'");
    }

    Ok(SshTarget {
        username,
        host,
        port,
    })
}

fn split_host_port(hostport: &str) -> Result<(String, Option<u16>)> {
    if let Some(rest) = hostport.strip_prefix('[') {
        let Some((host, after)) = rest.split_once(']') else {
            bail!("Invalid IPv6 target '{hostport}' (missing ']')");
        };
        if after.is_empty() {
            return Ok((host.to_string(), None));
        }
        let Some(port_str) = after.strip_prefix(':') else {
            bail!("Invalid IPv6 target '{hostport}'");
        };
        return Ok((host.to_string(), Some(parse_port(port_str)?)));
    }

    if let Some((host, port_str)) = hostport.rsplit_once(':') {
        if !host.contains(':')
            && !port_str.is_empty()
            && port_str.chars().all(|c| c.is_ascii_digit())
        {
            return Ok((host.to_string(), Some(parse_port(port_str)?)));
        }
    }

    Ok((hostport.to_string(), None))
}

fn parse_port(value: &str) -> Result<u16> {
    let port: u16 = value
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid port '{value}'"))?;
    if port == 0 {
        bail!("Port must be between 1 and 65535");
    }
    Ok(port)
}

#[cfg(test)]
mod tests {
    use super::parse_ssh_target;

    #[test]
    fn parses_user_host_port() {
        let target = parse_ssh_target("root@prod.example.com:2222").unwrap();
        assert_eq!(target.username.as_deref(), Some("root"));
        assert_eq!(target.host, "prod.example.com");
        assert_eq!(target.port, Some(2222));
    }

    #[test]
    fn parses_user_host_default_port() {
        let target = parse_ssh_target("ubuntu@10.0.0.8").unwrap();
        assert_eq!(target.username.as_deref(), Some("ubuntu"));
        assert_eq!(target.host, "10.0.0.8");
        assert_eq!(target.port, None);
    }

    #[test]
    fn parses_host_only() {
        let target = parse_ssh_target("db.internal").unwrap();
        assert!(target.username.is_none());
        assert_eq!(target.host, "db.internal");
        assert_eq!(target.port, None);
    }

    #[test]
    fn parses_bracket_ipv6() {
        let target = parse_ssh_target("root@[2001:db8::1]:22").unwrap();
        assert_eq!(target.host, "2001:db8::1");
        assert_eq!(target.port, Some(22));
    }
}
