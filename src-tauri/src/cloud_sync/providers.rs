use std::{collections::HashMap, time::Duration};

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use reqwest::{
    header::{ETAG, IF_MATCH, IF_NONE_MATCH},
    StatusCode,
};
use serde::{Deserialize, Serialize};

use super::{SyncEnvelope, SyncExchangeRequest, SyncExchangeResponse, SyncTransport};

const OBJECT_STORE_FORMAT: &str = "vibeshell-encrypted-sync";
const OBJECT_STORE_VERSION: u32 = 1;
const OBJECT_STORE_FILENAME: &str = "vibeshell-sync.json";
const MAX_OBJECT_STORE_BYTES: usize = 8 * 1024 * 1024;
const MAX_WRITE_ATTEMPTS: usize = 4;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SyncProviderKind {
    GithubGist,
    WebDav,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "provider", rename_all = "snake_case", deny_unknown_fields)]
pub enum SyncProviderConfig {
    GithubGist {
        gist_id: Option<String>,
        token: String,
    },
    WebDav {
        endpoint: String,
        username: String,
        password: String,
    },
}

impl SyncProviderConfig {
    pub fn kind(&self) -> SyncProviderKind {
        match self {
            Self::GithubGist { .. } => SyncProviderKind::GithubGist,
            Self::WebDav { .. } => SyncProviderKind::WebDav,
        }
    }

    pub fn target(&self) -> String {
        match self {
            Self::WebDav { endpoint, .. } => endpoint.clone(),
            Self::GithubGist { gist_id, .. } => gist_id
                .as_ref()
                .map(|id| format!("https://gist.github.com/{id}"))
                .unwrap_or_else(|| "GitHub Gist".to_string()),
        }
    }

    pub fn validate(&self) -> Result<()> {
        match self {
            Self::GithubGist { gist_id, token } => {
                if let Some(gist_id) = gist_id {
                    validate_gist_id(gist_id)?;
                }
                validate_token("GitHub token", token, 1, 1024)
            }
            Self::WebDav {
                endpoint,
                username,
                password,
            } => {
                normalize_https_url(endpoint, "WebDAV endpoint", true)?;
                if username.len() > 512
                    || username.chars().any(|value| matches!(value, '\r' | '\n'))
                {
                    bail!("Invalid WebDAV username");
                }
                validate_secret("WebDAV password", password, 0, 4096)
            }
        }
    }
}

pub struct MultiSyncTransport {
    client: reqwest::Client,
}

impl MultiSyncTransport {
    pub fn new() -> Result<Self> {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .user_agent(concat!("VibeShell/", env!("CARGO_PKG_VERSION")))
            .build()
            .context("Failed to build cloud sync HTTP client")?;
        Ok(Self { client })
    }

    async fn create_gist(&self, token: &str, vault_id: &str) -> Result<String> {
        let document = ObjectStoreDocument::new(vault_id);
        let content = serialize_document(&document)?;
        let response = self
            .client
            .post("https://api.github.com/gists")
            .bearer_auth(token)
            .json(&serde_json::json!({
                "description": "VibeShell end-to-end encrypted sync",
                "public": false,
                "files": {
                    OBJECT_STORE_FILENAME: { "content": content }
                }
            }))
            .send()
            .await
            .context("Failed to create GitHub Gist")?;
        let status = response.status();
        let body = read_limited_response(response, MAX_OBJECT_STORE_BYTES).await?;
        if !status.is_success() {
            bail!(
                "GitHub Gist creation returned HTTP {}: {}",
                status.as_u16(),
                response_excerpt(&body)
            );
        }
        let created: GistCreateResponse =
            serde_json::from_slice(&body).context("GitHub returned an invalid Gist response")?;
        validate_gist_id(&created.id)?;
        Ok(created.id)
    }

    async fn exchange_gist(
        &self,
        gist_id: &str,
        token: &str,
        vault_id: &str,
        request: &SyncExchangeRequest,
    ) -> Result<SyncExchangeResponse> {
        for _ in 0..MAX_WRITE_ATTEMPTS {
            let fetched = self.fetch_gist(gist_id, token, vault_id).await?;
            let (document, response, changed) =
                apply_object_exchange(fetched.document, vault_id, request)?;
            if !changed {
                return Ok(response);
            }
            let etag = fetched
                .etag
                .as_deref()
                .context("GitHub Gist response did not include an ETag for conditional update")?;
            if self.write_gist(gist_id, token, &document, etag).await? {
                return Ok(response);
            }
        }
        bail!("GitHub Gist changed repeatedly while syncing; retry the sync")
    }

    async fn fetch_gist(
        &self,
        gist_id: &str,
        token: &str,
        vault_id: &str,
    ) -> Result<FetchedDocument> {
        let response = self
            .client
            .get(format!("https://api.github.com/gists/{gist_id}"))
            .bearer_auth(token)
            .send()
            .await
            .context("Failed to read GitHub Gist")?;
        let status = response.status();
        let etag = response
            .headers()
            .get(ETAG)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let body = read_limited_response(response, MAX_OBJECT_STORE_BYTES).await?;
        if !status.is_success() {
            bail!(
                "GitHub Gist read returned HTTP {}: {}",
                status.as_u16(),
                response_excerpt(&body)
            );
        }
        let gist: GistResponse =
            serde_json::from_slice(&body).context("GitHub returned invalid Gist JSON")?;
        let document = match gist.files.get(OBJECT_STORE_FILENAME) {
            Some(file) if file.truncated => {
                let raw_url = file
                    .raw_url
                    .as_deref()
                    .context("GitHub truncated the sync Gist without a raw URL")?;
                self.fetch_raw_document(raw_url, token).await?
            }
            Some(file) => file
                .content
                .as_deref()
                .map(parse_document)
                .transpose()?
                .unwrap_or_else(|| ObjectStoreDocument::new(vault_id)),
            None => ObjectStoreDocument::new(vault_id),
        };
        Ok(FetchedDocument { document, etag })
    }

    async fn fetch_raw_document(&self, raw_url: &str, token: &str) -> Result<ObjectStoreDocument> {
        let parsed = reqwest::Url::parse(raw_url).context("GitHub returned an invalid raw URL")?;
        if parsed.scheme() != "https" || parsed.host_str() != Some("gist.githubusercontent.com") {
            bail!("GitHub returned an unexpected raw Gist URL");
        }
        let response = self
            .client
            .get(parsed)
            .bearer_auth(token)
            .send()
            .await
            .context("Failed to read raw GitHub Gist content")?;
        let status = response.status();
        let body = read_limited_response(response, MAX_OBJECT_STORE_BYTES).await?;
        if !status.is_success() {
            bail!("GitHub raw Gist read returned HTTP {}", status.as_u16());
        }
        parse_document(std::str::from_utf8(&body).context("Sync Gist is not UTF-8")?)
    }

    async fn write_gist(
        &self,
        gist_id: &str,
        token: &str,
        document: &ObjectStoreDocument,
        etag: &str,
    ) -> Result<bool> {
        let content = serialize_document(document)?;
        let mut request = self
            .client
            .patch(format!("https://api.github.com/gists/{gist_id}"))
            .bearer_auth(token)
            .json(&serde_json::json!({
                "files": {
                    OBJECT_STORE_FILENAME: { "content": content }
                }
            }));
        request = request.header(IF_MATCH, etag);
        let response = request
            .send()
            .await
            .context("Failed to update GitHub Gist")?;
        if matches!(
            response.status(),
            StatusCode::PRECONDITION_FAILED | StatusCode::CONFLICT
        ) {
            return Ok(false);
        }
        let status = response.status();
        if status.is_success() {
            return Ok(true);
        }
        let body = read_limited_response(response, MAX_OBJECT_STORE_BYTES).await?;
        bail!(
            "GitHub Gist update returned HTTP {}: {}",
            status.as_u16(),
            response_excerpt(&body)
        )
    }

    async fn exchange_webdav(
        &self,
        endpoint: &str,
        username: &str,
        password: &str,
        vault_id: &str,
        request: &SyncExchangeRequest,
    ) -> Result<SyncExchangeResponse> {
        for _ in 0..MAX_WRITE_ATTEMPTS {
            let fetched = self
                .fetch_webdav(endpoint, username, password, vault_id)
                .await?;
            let (document, response, changed) =
                apply_object_exchange(fetched.document, vault_id, request)?;
            if !changed {
                return Ok(response);
            }
            if self
                .write_webdav(
                    endpoint,
                    username,
                    password,
                    &document,
                    fetched.etag.as_deref(),
                )
                .await?
            {
                return Ok(response);
            }
        }
        bail!("WebDAV file changed repeatedly while syncing; retry the sync")
    }

    async fn fetch_webdav(
        &self,
        endpoint: &str,
        username: &str,
        password: &str,
        vault_id: &str,
    ) -> Result<FetchedDocument> {
        let response = with_basic_auth(self.client.get(endpoint), username, password)
            .send()
            .await
            .context("Failed to read WebDAV sync file")?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(FetchedDocument {
                document: ObjectStoreDocument::new(vault_id),
                etag: None,
            });
        }
        let status = response.status();
        let etag = response
            .headers()
            .get(ETAG)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let body = read_limited_response(response, MAX_OBJECT_STORE_BYTES).await?;
        if !status.is_success() {
            bail!(
                "WebDAV read returned HTTP {}: {}",
                status.as_u16(),
                response_excerpt(&body)
            );
        }
        let document =
            parse_document(std::str::from_utf8(&body).context("WebDAV sync file is not UTF-8")?)?;
        Ok(FetchedDocument { document, etag })
    }

    async fn write_webdav(
        &self,
        endpoint: &str,
        username: &str,
        password: &str,
        document: &ObjectStoreDocument,
        etag: Option<&str>,
    ) -> Result<bool> {
        let content = serialize_document(document)?;
        let mut request = with_basic_auth(
            self.client
                .put(endpoint)
                .header("content-type", "application/json")
                .body(content),
            username,
            password,
        );
        request = match etag {
            Some(etag) => request.header(IF_MATCH, etag),
            None => request.header(IF_NONE_MATCH, "*"),
        };
        let response = request
            .send()
            .await
            .context("Failed to write WebDAV sync file")?;
        if matches!(
            response.status(),
            StatusCode::PRECONDITION_FAILED | StatusCode::CONFLICT
        ) {
            return Ok(false);
        }
        if !response.status().is_success() {
            let status = response.status();
            let body = read_limited_response(response, MAX_OBJECT_STORE_BYTES).await?;
            bail!(
                "WebDAV write returned HTTP {}: {}",
                status.as_u16(),
                response_excerpt(&body)
            );
        }
        Ok(true)
    }
}

#[async_trait]
impl SyncTransport for MultiSyncTransport {
    async fn initialize(
        &self,
        config: &SyncProviderConfig,
        vault_id: &str,
    ) -> Result<SyncProviderConfig> {
        config.validate()?;
        match config {
            SyncProviderConfig::GithubGist {
                gist_id: None,
                token,
            } => Ok(SyncProviderConfig::GithubGist {
                gist_id: Some(self.create_gist(token, vault_id).await?),
                token: token.clone(),
            }),
            _ => Ok(config.clone()),
        }
    }

    async fn exchange(
        &self,
        config: &SyncProviderConfig,
        vault_id: &str,
        request: &SyncExchangeRequest,
    ) -> Result<SyncExchangeResponse> {
        config.validate()?;
        match config {
            SyncProviderConfig::GithubGist {
                gist_id: Some(gist_id),
                token,
            } => self.exchange_gist(gist_id, token, vault_id, request).await,
            SyncProviderConfig::GithubGist { gist_id: None, .. } => {
                bail!("GitHub Gist sync has not been initialized")
            }
            SyncProviderConfig::WebDav {
                endpoint,
                username,
                password,
            } => {
                self.exchange_webdav(endpoint, username, password, vault_id, request)
                    .await
            }
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ObjectStoreDocument {
    format: String,
    version: u32,
    vault_id: String,
    envelopes: Vec<SyncEnvelope>,
}

impl ObjectStoreDocument {
    fn new(vault_id: &str) -> Self {
        Self {
            format: OBJECT_STORE_FORMAT.to_string(),
            version: OBJECT_STORE_VERSION,
            vault_id: vault_id.to_string(),
            envelopes: Vec::new(),
        }
    }

    fn validate(&self, vault_id: &str) -> Result<()> {
        if self.format != OBJECT_STORE_FORMAT || self.version != OBJECT_STORE_VERSION {
            bail!("Unsupported VibeShell encrypted sync document")
        }
        if self.vault_id != vault_id {
            bail!("Sync provider contains data for a different vault")
        }
        if self.envelopes.len() > super::MAX_REMOTE_CHANGES_PER_PAGE {
            bail!("Sync provider contains too many encrypted envelopes")
        }
        Ok(())
    }
}

struct FetchedDocument {
    document: ObjectStoreDocument,
    etag: Option<String>,
}

#[derive(Deserialize)]
struct GistCreateResponse {
    id: String,
}

#[derive(Deserialize)]
struct GistResponse {
    files: HashMap<String, GistFile>,
}

#[derive(Deserialize)]
struct GistFile {
    content: Option<String>,
    #[serde(default)]
    truncated: bool,
    raw_url: Option<String>,
}

fn apply_object_exchange(
    mut document: ObjectStoreDocument,
    vault_id: &str,
    request: &SyncExchangeRequest,
) -> Result<(ObjectStoreDocument, SyncExchangeResponse, bool)> {
    document.validate(vault_id)?;
    let cursor = request
        .cursor
        .as_deref()
        .unwrap_or("0")
        .parse::<usize>()
        .context("Invalid sync provider cursor")?;
    if cursor > document.envelopes.len() {
        bail!("Sync provider cursor is ahead of the remote document")
    }

    let mut changed = false;
    if let Some(upload) = &request.envelope {
        match document
            .envelopes
            .iter()
            .find(|envelope| envelope.envelope_id == upload.envelope_id)
        {
            Some(existing) if existing != upload => {
                bail!("Encrypted sync envelope ID collision")
            }
            Some(_) => {}
            None => {
                document.envelopes.push(upload.clone());
                changed = true;
            }
        }
    }

    let response = SyncExchangeResponse {
        cursor: document.envelopes.len().to_string(),
        envelopes: document.envelopes.iter().skip(cursor).cloned().collect(),
        has_more: false,
    };
    Ok((document, response, changed))
}

fn parse_document(content: &str) -> Result<ObjectStoreDocument> {
    if content.len() > MAX_OBJECT_STORE_BYTES {
        bail!("Encrypted sync document exceeds the size limit")
    }
    serde_json::from_str(content).context("Sync provider contains invalid VibeShell JSON")
}

fn serialize_document(document: &ObjectStoreDocument) -> Result<String> {
    let content = serde_json::to_string(document)?;
    if content.len() > MAX_OBJECT_STORE_BYTES {
        bail!("Encrypted sync document exceeds the size limit")
    }
    Ok(content)
}

async fn read_limited_response(mut response: reqwest::Response, limit: usize) -> Result<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        bail!("Cloud sync response exceeds the size limit")
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .context("Failed to read cloud sync response")?
    {
        if body.len().saturating_add(chunk.len()) > limit {
            bail!("Cloud sync response exceeds the size limit")
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn with_basic_auth(
    request: reqwest::RequestBuilder,
    username: &str,
    password: &str,
) -> reqwest::RequestBuilder {
    if username.is_empty() && password.is_empty() {
        request
    } else {
        request.basic_auth(username, Some(password))
    }
}

fn normalize_https_url(value: &str, label: &str, allow_path: bool) -> Result<String> {
    let trimmed = value.trim().trim_end_matches('/');
    let parsed = reqwest::Url::parse(trimmed).with_context(|| format!("Invalid {label} URL"))?;
    if parsed.query().is_some()
        || parsed.fragment().is_some()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || (!allow_path && parsed.path() != "/" && !parsed.path().is_empty())
    {
        bail!("{label} URL contains unsupported credentials, path, query, or fragment")
    }
    let loopback = matches!(parsed.host_str(), Some("localhost" | "127.0.0.1" | "::1"));
    if parsed.scheme() != "https" && !(parsed.scheme() == "http" && loopback) {
        bail!("{label} must use HTTPS (HTTP is allowed only for loopback development)")
    }
    Ok(trimmed.to_string())
}

fn validate_gist_id(value: &str) -> Result<()> {
    if value.is_empty() || value.len() > 128 || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        bail!("Invalid GitHub Gist ID")
    }
    Ok(())
}

fn validate_secret(label: &str, value: &str, min: usize, max: usize) -> Result<()> {
    if !(min..=max).contains(&value.len())
        || value
            .chars()
            .any(|character| matches!(character, '\r' | '\n'))
    {
        bail!("Invalid {label}")
    }
    Ok(())
}

fn validate_token(label: &str, value: &str, min: usize, max: usize) -> Result<()> {
    validate_secret(label, value, min, max)?;
    if value.chars().any(char::is_whitespace) {
        bail!("Invalid {label}")
    }
    Ok(())
}

fn response_excerpt(body: &[u8]) -> String {
    String::from_utf8_lossy(body)
        .chars()
        .take(300)
        .collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_exchange_appends_once_and_returns_changes_after_cursor() {
        let first = SyncEnvelope {
            envelope_id: "first".to_string(),
            ciphertext: "encrypted-1".to_string(),
        };
        let second = SyncEnvelope {
            envelope_id: "second".to_string(),
            ciphertext: "encrypted-2".to_string(),
        };
        let mut document = ObjectStoreDocument::new("vault-a");
        document.envelopes.push(first.clone());

        let (document, response, changed) = apply_object_exchange(
            document,
            "vault-a",
            &SyncExchangeRequest {
                cursor: Some("1".to_string()),
                envelope: Some(second.clone()),
            },
        )
        .unwrap();

        assert!(changed);
        assert_eq!(response.cursor, "2");
        assert_eq!(response.envelopes, vec![second.clone()]);

        let (_, replay, changed) = apply_object_exchange(
            document,
            "vault-a",
            &SyncExchangeRequest {
                cursor: Some("2".to_string()),
                envelope: Some(second),
            },
        )
        .unwrap();
        assert!(!changed);
        assert!(replay.envelopes.is_empty());
    }

    #[test]
    fn object_exchange_rejects_another_vault_and_divergent_envelope_replay() {
        let document = ObjectStoreDocument::new("vault-a");
        assert!(apply_object_exchange(
            document,
            "vault-b",
            &SyncExchangeRequest {
                cursor: None,
                envelope: None,
            },
        )
        .is_err());

        let mut document = ObjectStoreDocument::new("vault-a");
        document.envelopes.push(SyncEnvelope {
            envelope_id: "same-id".to_string(),
            ciphertext: "first".to_string(),
        });
        assert!(apply_object_exchange(
            document,
            "vault-a",
            &SyncExchangeRequest {
                cursor: None,
                envelope: Some(SyncEnvelope {
                    envelope_id: "same-id".to_string(),
                    ciphertext: "different".to_string(),
                }),
            },
        )
        .is_err());
    }

    #[test]
    fn provider_validation_rejects_insecure_remote_endpoints() {
        let webdav = SyncProviderConfig::WebDav {
            endpoint: "http://dav.example.com/vibeshell.json".to_string(),
            username: "rick".to_string(),
            password: "secret".to_string(),
        };
        assert!(webdav.validate().is_err());

        let local = SyncProviderConfig::WebDav {
            endpoint: "http://127.0.0.1:8080/vibeshell.json".to_string(),
            username: String::new(),
            password: String::new(),
        };
        assert!(local.validate().is_ok());
    }
}
