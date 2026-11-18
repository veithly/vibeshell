use crate::{
    network::{ProxySettings, ProxySettingsInput, ProxySettingsView, Route},
    storage::Database,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};
use tauri::State;
#[derive(Default)]
struct RequestRegistry {
    active: HashMap<String, tokio::sync::oneshot::Sender<()>>,
    early_cancelled: HashMap<String, Instant>,
}
type Cancellations = Mutex<RequestRegistry>;
static CANCELLATIONS: OnceLock<Cancellations> = OnceLock::new();

struct RequestGuard(String);
impl Drop for RequestGuard {
    fn drop(&mut self) {
        CANCELLATIONS
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .active
            .remove(&self.0);
    }
}

#[tauri::command]
pub fn get_network_proxy(db: State<'_, Arc<Database>>) -> Result<ProxySettingsView, String> {
    ProxySettings::load(&db)
        .map(|s| s.view())
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub fn set_network_proxy(
    db: State<'_, Arc<Database>>,
    settings: ProxySettingsInput,
) -> Result<ProxySettingsView, String> {
    ProxySettings::save(&db, settings)
        .map(|s| s.view())
        .map_err(|e| e.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NetworkRequest {
    #[serde(default)]
    pub image: bool,
    pub id: String,
    pub url: String,
    pub method: String,
    pub headers: HashMap<String, String>,
    pub body: Option<String>,
}
#[derive(Serialize)]
pub struct NetworkResponse {
    status: u16,
    body: String,
}

#[tauri::command]
pub fn network_cancel(id: String) {
    if id.is_empty() || id.len() > 80 {
        return;
    }
    let mut registry = CANCELLATIONS.get_or_init(Default::default).lock().unwrap();
    if let Some(sender) = registry.active.remove(&id) {
        let _ = sender.send(());
    } else {
        registry
            .early_cancelled
            .retain(|_, at| at.elapsed() < Duration::from_secs(30));
        if registry.early_cancelled.len() < 64 {
            registry.early_cancelled.insert(id, Instant::now());
        }
    }
}

#[tauri::command]
pub async fn network_request(request: NetworkRequest) -> Result<NetworkResponse, String> {
    let url = reqwest::Url::parse(&request.url).map_err(|_| "Invalid HTTP destination")?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("HTTP destinations must not contain URL credentials".into());
    }
    if request.id.is_empty()
        || request.id.len() > 80
        || request.body.as_ref().is_some_and(|v| v.len() > 1024 * 1024)
        || request.headers.len() > 64
        || request
            .headers
            .iter()
            .map(|(k, v)| k.len() + v.len())
            .sum::<usize>()
            > 32 * 1024
    {
        return Err("HTTP request exceeds limit".into());
    }
    let method = match request.method.as_str() {
        "GET" => reqwest::Method::GET,
        "POST" => reqwest::Method::POST,
        _ => return Err("Only GET and POST are supported".into()),
    };
    let route = Route::current().map_err(|e| e.to_string())?;
    let client = route
        .configure_http(reqwest::Client::builder())
        .user_agent(concat!("VibeShell/", env!("CARGO_PKG_VERSION")))
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|_| "Cannot build routed HTTP client")?;
    let mut builder = client.request(method, url);
    for (key, value) in request.headers {
        builder = builder.header(key, value);
    }
    if let Some(body) = request.body {
        builder = builder.body(body);
    }
    let (sender, receiver) = tokio::sync::oneshot::channel();
    {
        let mut pending = CANCELLATIONS.get_or_init(Default::default).lock().unwrap();
        pending
            .early_cancelled
            .retain(|_, at| at.elapsed() < Duration::from_secs(30));
        if pending.early_cancelled.remove(&request.id).is_some() {
            return Err("HTTP request cancelled".into());
        }
        if pending.active.len() >= 32 || pending.active.contains_key(&request.id) {
            return Err("Too many pending HTTP requests or duplicate request ID".into());
        }
        pending.active.insert(request.id.clone(), sender);
    }
    let _guard = RequestGuard(request.id);
    let operation = async {
        // reqwest errors may carry token-bearing URLs; do not serialize them.
        let mut response = builder.send().await.map_err(|_| "HTTP request failed on configured route (proxy, TLS, timeout or connection error); no direct fallback")?;
        let status = response.status().as_u16();
        let image_mime = if request.image {
            if !(200..300).contains(&status) {
                return Err(format!("Remote image request failed (HTTP {status})"));
            }
            let mime = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default()
                .split(';')
                .next()
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase();
            if !matches!(
                mime.as_str(),
                "image/png" | "image/jpeg" | "image/webp" | "image/gif" | "image/avif"
            ) {
                return Err("Remote image must be PNG, JPEG, WebP, GIF or AVIF; no direct WebView fallback is used".into());
            }
            Some(mime)
        } else {
            None
        };
        let limit = if request.image {
            8 * 1024 * 1024
        } else {
            2 * 1024 * 1024
        };
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "HTTP response read failed")?
        {
            if bytes.len() + chunk.len() > limit {
                return Err("HTTP response exceeds its bounded size limit".to_string());
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(NetworkResponse {
            status,
            body: if let Some(mime) = image_mime {
                use base64::Engine;
                format!(
                    "data:{mime};base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(bytes)
                )
            } else {
                String::from_utf8(bytes).map_err(|_| "HTTP response is not UTF-8")?
            },
        })
    };
    tokio::select! { result = operation => result, _ = receiver => Err("HTTP request cancelled".into()) }
}

/// Keep the updater's own signature verification and installation, but create
/// its HTTP clients with the same policy. No proxy password crosses to JS.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn update_timestamp_rfc3339(seconds: i64, nanoseconds: u32) -> Result<String, String> {
    // Match the updater plugin's JS metadata contract. OffsetDateTime's Display
    // output is not RFC 3339 and is not portable across WebView date parsers.
    chrono::DateTime::from_timestamp(seconds, nanoseconds)
        .map(|date| date.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true))
        .ok_or_else(|| "Invalid signed update timestamp".into())
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub async fn network_check_update(
    webview: tauri::Webview,
) -> Result<Option<serde_json::Value>, String> {
    use tauri::Manager;
    use tauri_plugin_updater::UpdaterExt;
    let route = Route::current().map_err(|e| e.to_string())?;
    let updater = webview
        .updater_builder()
        .timeout(Duration::from_secs(30))
        .configure_client(move |builder| route.configure_http(builder))
        .build()
        .map_err(|_| "Cannot initialize signed updater")?;
    let Some(update) = updater
        .check()
        .await
        .map_err(|_| "Signed update check failed on configured network route")?
    else {
        return Ok(None);
    };
    let date = update
        .date
        .map(|date| update_timestamp_rfc3339(date.unix_timestamp(), date.nanosecond()))
        .transpose()?;
    let mut metadata = serde_json::json!({ "rid":0, "currentVersion":update.current_version, "version":update.version,
        "date":date, "body":update.body, "rawJson":update.raw_json });
    metadata["rid"] = serde_json::json!(webview.resources_table().add(update));
    Ok(Some(metadata))
}
#[cfg(any(target_os = "android", target_os = "ios"))]
#[tauri::command]
pub async fn network_check_update() -> Result<Option<serde_json::Value>, String> {
    Err("Signed desktop updater is unavailable on mobile".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    fn signed_update_dates_retain_the_js_rfc3339_contract() {
        let date = update_timestamp_rfc3339(0, 123_456_789).unwrap();
        assert_eq!(date, "1970-01-01T00:00:00.123456789Z");
        let parsed = chrono::DateTime::parse_from_rfc3339(&date).unwrap();
        assert_eq!(parsed.timestamp_nanos_opt(), Some(123_456_789));
        assert!(update_timestamp_rfc3339(i64::MAX, 0).is_err());
    }

    #[tokio::test]
    async fn native_http_keeps_api_headers_and_identifies_the_application() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let server = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let url = format!("http://{}/releases/latest", server.local_addr().unwrap());
        let peer = tokio::spawn(async move {
            let (mut socket, _) = server.accept().await.unwrap();
            let mut head = Vec::new();
            while !head.ends_with(b"\r\n\r\n") {
                assert!(head.len() < 16_384);
                head.push(socket.read_u8().await.unwrap());
            }
            socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}").await.unwrap();
            String::from_utf8(head).unwrap().to_ascii_lowercase()
        });
        let response = network_request(NetworkRequest {
            image: false,
            id: uuid::Uuid::new_v4().to_string(),
            url,
            method: "GET".into(),
            headers: HashMap::from([("Accept".into(), "application/vnd.github+json".into())]),
            body: None,
        })
        .await
        .unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.body, "{}");
        let head = peer.await.unwrap();
        assert!(head.contains("accept: application/vnd.github+json\r\n"));
        assert!(head.contains(&format!("user-agent: vibeshell/{}\r\n", env!("CARGO_PKG_VERSION"))),
            "The native HTTP bridge must identify VibeShell; GitHub rejects missing User-Agent headers");
    }

    #[tokio::test]
    async fn early_and_inflight_cancellation_are_bounded_and_release_slots() {
        let id = uuid::Uuid::new_v4().to_string();
        network_cancel(id.clone());
        let request = |id: String, url: String| NetworkRequest {
            image: false,
            id,
            url,
            method: "GET".into(),
            headers: HashMap::new(),
            body: None,
        };
        let result = network_request(request(id, "http://127.0.0.1:1/".into())).await;
        assert!(matches!(result, Err(message) if message.contains("cancelled")));
        let server = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let task = tokio::spawn(network_request(request(
            id.clone(),
            format!("http://{}/", server.local_addr().unwrap()),
        )));
        let (mut connection, _) = server.accept().await.unwrap();
        let mut bytes = [0; 1024];
        use tokio::io::AsyncReadExt;
        assert!(connection.read(&mut bytes).await.unwrap() > 0);
        network_cancel(id.clone());
        let result = tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(result, Err(message) if message.contains("cancelled")));
        assert!(!CANCELLATIONS
            .get()
            .unwrap()
            .lock()
            .unwrap()
            .active
            .contains_key(&id));
    }
}
