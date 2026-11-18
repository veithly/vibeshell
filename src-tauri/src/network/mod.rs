//! One explicit outbound policy for native HTTP and raw TCP. Proxy secrets
//! remain encrypted in the device-local database, never in status responses.
mod dial;
#[cfg(test)]
#[path = "tests.rs"]
mod transport_tests;
use crate::storage::{Crypto, Database};
use anyhow::{anyhow, ensure, Context, Result};
pub use dial::{connect, BoxStream};
use serde::{Deserialize, Serialize};
use std::{
    sync::{Arc, OnceLock},
    time::Duration,
};

const SETTINGS_KEY: &str = "outbound_proxy_v1";
static DATABASE: OnceLock<Arc<Database>> = OnceLock::new();
pub fn initialize(database: Arc<Database>) {
    let _ = DATABASE.set(database);
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProxyMode {
    #[default]
    Direct,
    System,
    Custom,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProxySettings {
    pub mode: ProxyMode,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    encrypted_password: String,
}
impl std::fmt::Debug for ProxySettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ProxySettings { credentials: [REDACTED] }")
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProxySettingsInput {
    pub mode: ProxyMode,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub username: String,
    /// None preserves the encrypted value. Some("") explicitly removes it.
    pub password: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxySettingsView {
    pub mode: ProxyMode,
    pub url: String,
    pub username: String,
    pub has_password: bool,
}

impl ProxySettings {
    pub fn load(database: &Database) -> Result<Self> {
        let config: Self = database
            .get_setting(SETTINGS_KEY)?
            .map(|s| serde_json::from_str(&s))
            .transpose()
            .context("Saved network configuration is invalid; refusing direct fallback")?
            .unwrap_or_default();
        if config.mode == ProxyMode::Custom {
            validate_url(&config.url, false)?;
        }
        Ok(config)
    }
    pub fn view(&self) -> ProxySettingsView {
        ProxySettingsView {
            mode: self.mode,
            url: self.url.clone(),
            username: self.username.clone(),
            has_password: !self.encrypted_password.is_empty(),
        }
    }
    pub fn save(database: &Database, mut input: ProxySettingsInput) -> Result<Self> {
        let old = Self::load(database)?;
        if input.mode != ProxyMode::Custom && input.url.is_empty() && input.username.is_empty() {
            input.url = old.url.clone();
            input.username = old.username.clone();
        }
        ensure!(
            input.username.len() <= 255
                && !input.username.chars().any(char::is_control)
                && !input.username.contains(':'),
            "Invalid proxy username"
        );
        let url = input.url.trim().to_string();
        if input.mode == ProxyMode::Custom {
            validate_url(&url, false)?;
        }
        let encrypted_password = match input.password {
            Some(password) if password.is_empty() => String::new(),
            Some(password) => {
                ensure!(password.len() <= 255, "Proxy password exceeds 255 bytes");
                Crypto::device()?.encrypt_base64(password.as_bytes())?
            }
            None => {
                ensure!(old.encrypted_password.is_empty() || (old.url == url && old.username == input.username), "Changing the proxy endpoint or username requires explicitly replacing or clearing its saved password");
                old.encrypted_password
            }
        };
        if input.mode == ProxyMode::Custom && url.starts_with("socks4") {
            ensure!(
                encrypted_password.is_empty(),
                "SOCKS4 supports a user ID, not password authentication; use SOCKS5 for passwords"
            );
        }
        let config = Self {
            mode: input.mode,
            url,
            username: input.username,
            encrypted_password,
        };
        database.set_setting(SETTINGS_KEY, &serde_json::to_string(&config)?)?;
        Ok(config)
    }
    fn authenticated_url(&self) -> Result<reqwest::Url> {
        let mut url = validate_url(&self.url, false)?;
        if !self.username.is_empty() {
            url.set_username(&self.username)
                .map_err(|_| anyhow!("Invalid proxy username"))?;
        }
        if !self.encrypted_password.is_empty() {
            let password = Crypto::device()?
                .decrypt_base64(&self.encrypted_password)
                .context("Cannot decrypt the saved proxy password")?;
            let password = String::from_utf8(password).context("Invalid proxy secret encoding")?;
            url.set_password(Some(&password))
                .map_err(|_| anyhow!("Invalid proxy password"))?;
        }
        Ok(url)
    }
}

pub fn current() -> Result<ProxySettings> {
    DATABASE
        .get()
        .map(|db| ProxySettings::load(db))
        .unwrap_or_else(|| Ok(ProxySettings::default()))
}

pub(crate) fn validate_url(raw: &str, allow_credentials: bool) -> Result<reqwest::Url> {
    // Do not include a user-supplied URL in errors: it may contain credentials.
    let url = reqwest::Url::parse(raw).map_err(|_| anyhow!("Invalid proxy URL"))?;
    ensure!(
        matches!(
            url.scheme(),
            "http" | "https" | "socks4" | "socks4a" | "socks5" | "socks5h"
        ),
        "Proxy scheme must be http, https, socks4, socks4a, socks5 or socks5h"
    );
    ensure!(
        url.host_str().is_some()
            && url
                .port_or_known_default()
                .or_else(|| url.scheme().starts_with("socks").then_some(1080))
                .is_some_and(|p| p != 0),
        "Proxy host and valid port are required"
    );
    ensure!(
        url.path().is_empty() || url.path() == "/",
        "Proxy URL cannot include a path"
    );
    ensure!(
        url.query().is_none() && url.fragment().is_none(),
        "Proxy URL cannot contain query or fragment data"
    );
    ensure!(
        allow_credentials || (url.username().is_empty() && url.password().is_none()),
        "Do not put proxy credentials in URLs; use username and protected password input"
    );
    ensure!(
        !url.scheme().starts_with("socks4") || url.password().is_none(),
        "SOCKS4 does not support password authentication; use SOCKS5"
    );
    Ok(url)
}

/// Integrated external transports cannot promise to obey the native dialer.
/// Refuse instead of letting an installed tsh silently bypass a chosen route.
pub fn require_native_route_compatible_transport(name: &str) -> Result<()> {
    ensure!(!Route::current()?.has_proxy(), "{name} cannot enforce VibeShell's selected outbound proxy. No process was started and no direct fallback is allowed. Use a native SSH connection or explicitly select Direct before using this external transport.");
    Ok(())
}

/// Snapshot the environment once per operation. System means the standard
/// ALL_PROXY/HTTPS_PROXY/HTTP_PROXY and NO_PROXY variables, not OS UI settings.
#[derive(Clone)]
pub struct Route {
    #[cfg(test)]
    test_roots: Vec<tokio_rustls::rustls::pki_types::CertificateDer<'static>>,
    http: Option<reqwest::Url>,
    https: Option<reqwest::Url>,
    tcp: Option<reqwest::Url>,
    bypass: Vec<String>,
}
impl Route {
    pub fn from_settings(config: &ProxySettings) -> Result<Self> {
        let mut route = Self {
            #[cfg(test)]
            test_roots: vec![],
            http: None,
            https: None,
            tcp: None,
            bypass: vec![],
        };
        match config.mode {
            ProxyMode::Direct => {}
            ProxyMode::Custom => {
                let url = config.authenticated_url()?;
                route.http = Some(url.clone());
                route.https = Some(url.clone());
                route.tcp = Some(url);
            }
            ProxyMode::System => {
                let env = |key: &str| -> Option<String> {
                    std::env::var(key.to_ascii_lowercase())
                        .ok()
                        .or_else(|| std::env::var(key).ok())
                        .filter(|s| !s.is_empty())
                };
                let all = env("ALL_PROXY")
                    .map(|s| validate_url(&s, true))
                    .transpose()?;
                let http = env("HTTP_PROXY")
                    .map(|s| validate_url(&s, true))
                    .transpose()?;
                let https = env("HTTPS_PROXY")
                    .map(|s| validate_url(&s, true))
                    .transpose()?;
                route.http = http.clone().or_else(|| all.clone());
                route.https = https.clone().or_else(|| all.clone());
                route.tcp = all.or(https).or(http);
                route.bypass = env("NO_PROXY")
                    .unwrap_or_default()
                    .split(',')
                    .map(|s| s.trim().to_ascii_lowercase())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
        }
        Ok(route)
    }
    pub fn current() -> Result<Self> {
        Self::from_settings(&current()?)
    }
    fn excluded(&self, host: &str) -> bool {
        let host = host.trim_matches(['[', ']']).to_ascii_lowercase();
        self.bypass.iter().any(|pattern| {
            let domain = pattern.trim_start_matches('.');
            pattern == "*" || host == domain || host.ends_with(&format!(".{domain}"))
        })
    }
    pub(crate) fn tcp_proxy(&self, host: &str) -> Option<&reqwest::Url> {
        if self.excluded(host) {
            None
        } else {
            self.tcp.as_ref()
        }
    }
    pub fn has_proxy(&self) -> bool {
        self.http.is_some() || self.https.is_some() || self.tcp.is_some()
    }
    pub fn configure_http(&self, builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
        let route = self.clone();
        builder.no_proxy().proxy(reqwest::Proxy::custom(move |url| {
            if route.excluded(url.host_str().unwrap_or_default()) {
                return None;
            }
            if url.scheme() == "https" {
                route.https.clone()
            } else {
                route.http.clone()
            }
        }))
    }
    pub fn client(&self) -> Result<reqwest::Client> {
        self.configure_http(reqwest::Client::builder())
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(30))
            .user_agent(concat!("VibeShell/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| anyhow!("Cannot build HTTP client for the configured network route"))
    }
}

pub fn http_client() -> Result<reqwest::Client> {
    Route::current()?.client()
}

/// A scoped, single-connection adapter for native clients that cannot accept a
/// custom stream (MySQL). The proxy handshake happens before local credentials.
pub struct TcpBridge {
    pub port: u16,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for TcpBridge {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl TcpBridge {
    pub async fn start(host: &str, port: u16) -> Result<Self> {
        let mut upstream = connect(&Route::current()?, host, port).await?;
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
        let port = listener.local_addr()?.port();
        let task = tokio::spawn(async move {
            if let Ok(Ok((mut local, _))) =
                tokio::time::timeout(Duration::from_secs(10), listener.accept()).await
            {
                drop(listener);
                let _ = tokio::io::copy_bidirectional(&mut local, &mut upstream).await;
            }
        });
        Ok(Self { port, task })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_preserve_secret_bytes_and_never_return_or_log_password() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new_at(dir.path().join("proxy.db")).unwrap();
        let input = ProxySettingsInput {
            mode: ProxyMode::Custom,
            url: "socks5h://proxy.invalid:1080".into(),
            username: "user".into(),
            password: Some(" secret \n".into()),
        };
        let saved = ProxySettings::save(&db, input).unwrap();
        assert!(!db
            .get_setting(SETTINGS_KEY)
            .unwrap()
            .unwrap()
            .contains("secret"));
        assert!(!format!("{saved:?}").contains("secret"));
        assert!(!serde_json::to_string(&saved.view())
            .unwrap()
            .contains("secret"));
        let loaded = ProxySettings::load(&db).unwrap();
        let raw = loaded.authenticated_url().unwrap();
        assert_eq!(
            percent_encoding::percent_decode_str(raw.password().unwrap())
                .decode_utf8()
                .unwrap(),
            " secret \n"
        );
        assert!(ProxySettings::save(
            &db,
            ProxySettingsInput {
                mode: ProxyMode::Custom,
                url: "http://different.invalid:8080".into(),
                username: "user".into(),
                password: None
            }
        )
        .is_err());
    }
    #[test]
    fn malformed_proxies_and_url_credentials_fail_closed() {
        for bad in [
            "http://u:secret@host",
            "file:///tmp/a",
            "ftp://host",
            "socks5://host:0",
            "http://host/path",
            "https://host/?secret=x",
        ] {
            assert!(validate_url(bad, false).is_err());
        }
        for good in [
            "http://host:8080",
            "https://host",
            "socks4://host:1080",
            "socks4a://host",
            "socks5://[::1]:1080",
            "socks5h://host",
        ] {
            assert!(validate_url(good, false).is_ok(), "{good}");
        }
    }
}
