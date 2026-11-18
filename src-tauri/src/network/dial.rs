use super::Route;
use anyhow::{anyhow, bail, ensure, Context, Result};
use base64::Engine;
use std::{
    net::{IpAddr, Ipv4Addr},
    sync::Arc,
    time::Duration,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
pub trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Stream for T {}
pub type BoxStream = Box<dyn Stream>;

/// Dropping this future drops its socket (cancellation). Every DNS, TCP, TLS
/// and protocol handshake is covered by one deadline. Never retry directly.
pub async fn connect(route: &Route, host: &str, port: u16) -> Result<BoxStream> {
    ensure!(
        !host.is_empty()
            && host.len() <= 253
            && !host.chars().any(|c| c.is_control() || c.is_whitespace())
            && port != 0,
        "Invalid outbound host or port"
    );
    tokio::time::timeout(Duration::from_secs(15), connect_inner(route, host, port))
        .await
        .map_err(|_| anyhow!("Network route timed out after 15 seconds; no direct fallback"))?
}

async fn connect_inner(route: &Route, host: &str, port: u16) -> Result<BoxStream> {
    let Some(proxy) = route.tcp_proxy(host) else {
        return Ok(Box::new(
            tokio::net::TcpStream::connect((host, port))
                .await
                .context("Direct TCP connection failed")?,
        ));
    };
    let proxy_host = proxy
        .host_str()
        .ok_or_else(|| anyhow!("Proxy host missing"))?
        .trim_matches(['[', ']']);
    let proxy_port = proxy.port_or_known_default().unwrap_or(1080);
    let socket = tokio::net::TcpStream::connect((proxy_host, proxy_port))
        .await
        .map_err(|_| anyhow!("Cannot connect to configured proxy; no direct fallback"))?;
    let mut stream: BoxStream = if proxy.scheme() == "https" {
        use tokio_rustls::rustls::{pki_types::ServerName, ClientConfig, RootCertStore};
        let roots = RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        };
        #[cfg(test)]
        let roots = {
            let mut roots = roots;
            for root in &route.test_roots {
                roots.add(root.clone())?;
            }
            roots
        };
        let tls = ClientConfig::builder_with_provider(Arc::new(
            tokio_rustls::rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()?
        .with_root_certificates(roots)
        .with_no_client_auth();
        let name = ServerName::try_from(proxy_host.to_owned())
            .map_err(|_| anyhow!("Invalid HTTPS proxy hostname"))?;
        Box::new(
            tokio_rustls::TlsConnector::from(Arc::new(tls))
                .connect(name, socket)
                .await
                .map_err(|_| {
                    anyhow!("HTTPS proxy TLS verification failed; no insecure or direct fallback")
                })?,
        )
    } else {
        Box::new(socket)
    };
    let user = decode(proxy.username())?;
    let password = proxy.password().map(decode).transpose()?;
    match proxy.scheme() {
        "http" | "https" => {
            http_connect(&mut stream, host, port, &user, password.as_deref()).await?
        }
        "socks4" | "socks4a" => {
            socks4(&mut stream, host, port, &user, proxy.scheme() == "socks4a").await?
        }
        "socks5" | "socks5h" => {
            socks5(
                &mut stream,
                host,
                port,
                &user,
                password.as_deref(),
                proxy.scheme() == "socks5h",
            )
            .await?
        }
        _ => bail!("Unsupported proxy protocol"),
    }
    Ok(stream)
}
fn decode(s: &str) -> Result<String> {
    percent_encoding::percent_decode_str(s)
        .decode_utf8()
        .map(|s| s.into_owned())
        .map_err(|_| anyhow!("Invalid proxy credential encoding"))
}

async fn http_connect(
    stream: &mut BoxStream,
    host: &str,
    port: u16,
    user: &str,
    password: Option<&str>,
) -> Result<()> {
    let authority = if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };
    let auth = if !user.is_empty() || password.is_some() {
        format!(
            "Proxy-Authorization: Basic {}\r\n",
            base64::engine::general_purpose::STANDARD
                .encode(format!("{user}:{}", password.unwrap_or_default()))
        )
    } else {
        String::new()
    };
    stream
        .write_all(
            format!("CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\n{auth}\r\n").as_bytes(),
        )
        .await?;
    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") {
        ensure!(
            head.len() < 16384,
            "HTTP proxy response headers exceeded limit"
        );
        head.push(
            stream
                .read_u8()
                .await
                .context("HTTP proxy closed during CONNECT")?,
        );
    }
    let first = std::str::from_utf8(&head)
        .map_err(|_| anyhow!("Invalid HTTP proxy response"))?
        .lines()
        .next()
        .unwrap_or_default();
    let parts: Vec<_> = first.split_whitespace().collect();
    let code = parts
        .get(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);
    ensure!(
        parts
            .first()
            .is_some_and(|p| matches!(*p, "HTTP/1.0" | "HTTP/1.1")),
        "Invalid HTTP proxy protocol response"
    );
    if code == 407 {
        bail!("Proxy authentication was rejected (HTTP 407)");
    }
    ensure!(
        (200..300).contains(&code),
        "HTTP proxy refused CONNECT (status {code}); no direct fallback"
    );
    Ok(())
}
async fn resolve(host: &str, port: u16) -> Result<IpAddr> {
    if let Ok(ip) = host.parse() {
        return Ok(ip);
    }
    tokio::net::lookup_host((host, port))
        .await?
        .next()
        .map(|s| s.ip())
        .ok_or_else(|| anyhow!("Destination DNS returned no addresses"))
}
async fn socks4(
    stream: &mut BoxStream,
    host: &str,
    port: u16,
    user: &str,
    remote_dns: bool,
) -> Result<()> {
    ensure!(
        !user.contains('\0') && user.len() <= 255,
        "Invalid SOCKS4 user ID"
    );
    let ip = if remote_dns {
        Ipv4Addr::new(0, 0, 0, 1)
    } else {
        match resolve(host, port).await? {
            IpAddr::V4(ip) => ip,
            IpAddr::V6(_) => bail!("SOCKS4 cannot represent IPv6 destinations; use SOCKS5"),
        }
    };
    let mut bytes = vec![4, 1];
    bytes.extend(port.to_be_bytes());
    bytes.extend(ip.octets());
    bytes.extend(user.as_bytes());
    bytes.push(0);
    if remote_dns {
        ensure!(
            host.parse::<std::net::Ipv6Addr>().is_err(),
            "SOCKS4a cannot represent IPv6; use SOCKS5h"
        );
        bytes.extend(host.as_bytes());
        bytes.push(0);
    }
    stream.write_all(&bytes).await?;
    let mut reply = [0; 8];
    stream.read_exact(&mut reply).await?;
    ensure!(
        reply[0] == 0 && reply[1] == 90,
        "SOCKS4 proxy rejected CONNECT; no direct fallback"
    );
    Ok(())
}
async fn socks5(
    stream: &mut BoxStream,
    host: &str,
    port: u16,
    user: &str,
    password: Option<&str>,
    remote_dns: bool,
) -> Result<()> {
    let authenticated = !user.is_empty() || password.is_some();
    let method = if authenticated { 2 } else { 0 };
    stream.write_all(&[5, 1, method]).await?;
    let mut reply = [0; 2];
    stream.read_exact(&mut reply).await?;
    ensure!(
        reply == [5, method],
        "SOCKS5 authentication method rejected"
    );
    if authenticated {
        let password = password.unwrap_or_default();
        ensure!(
            !user.is_empty() && user.len() <= 255 && !password.is_empty() && password.len() <= 255,
            "SOCKS5 username/password must each contain 1–255 bytes"
        );
        let mut auth = vec![1, user.len() as u8];
        auth.extend(user.as_bytes());
        auth.push(password.len() as u8);
        auth.extend(password.as_bytes());
        stream.write_all(&auth).await?;
        stream.read_exact(&mut reply).await?;
        ensure!(reply == [1, 0], "SOCKS5 credentials rejected");
    }
    let mut request = vec![5, 1, 0];
    let ip = host.parse::<IpAddr>().ok();
    if remote_dns && ip.is_none() {
        ensure!(host.len() <= 255, "SOCKS5 hostname too long");
        request.extend([3, host.len() as u8]);
        request.extend(host.as_bytes());
    } else {
        match match ip {
            Some(ip) => ip,
            None => resolve(host, port).await?,
        } {
            IpAddr::V4(ip) => {
                request.push(1);
                request.extend(ip.octets());
            }
            IpAddr::V6(ip) => {
                request.push(4);
                request.extend(ip.octets());
            }
        }
    }
    request.extend(port.to_be_bytes());
    stream.write_all(&request).await?;
    let mut header = [0; 4];
    stream.read_exact(&mut header).await?;
    ensure!(
        header[0] == 5 && header[1] == 0 && header[2] == 0,
        "SOCKS5 proxy refused CONNECT; no direct fallback"
    );
    let length = match header[3] {
        1 => 4,
        4 => 16,
        3 => stream.read_u8().await? as usize,
        _ => bail!("Invalid SOCKS5 reply address type"),
    };
    let mut tail = vec![0; length + 2];
    stream.read_exact(&mut tail).await?;
    Ok(())
}
