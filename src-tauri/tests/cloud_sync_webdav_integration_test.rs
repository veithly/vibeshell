use std::sync::{Arc, Mutex};

use axum::{
    body::Bytes,
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
    routing::get,
    Router,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use vibeshell_core::{
    cloud_sync::CloudSyncManager,
    storage::{AuthType, Database, Server},
};

#[derive(Clone, Default)]
struct WebDavState {
    document: Arc<Mutex<Option<(String, String)>>>,
}

async fn read_document(State(state): State<WebDavState>, headers: HeaderMap) -> impl IntoResponse {
    if !authorized(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let document = state.document.lock().unwrap();
    match document.as_ref() {
        Some((etag, body)) => ([(header::ETAG, etag.as_str())], body.clone()).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn write_document(
    State(state): State<WebDavState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    if !authorized(&headers) {
        return StatusCode::UNAUTHORIZED;
    }
    let mut document = state.document.lock().unwrap();
    let precondition_matches = match document.as_ref() {
        Some((etag, _)) => {
            headers
                .get(header::IF_MATCH)
                .and_then(|value| value.to_str().ok())
                == Some(etag.as_str())
        }
        None => {
            headers
                .get(header::IF_NONE_MATCH)
                .and_then(|value| value.to_str().ok())
                == Some("*")
        }
    };
    if !precondition_matches {
        return StatusCode::PRECONDITION_FAILED;
    }
    let next_etag = format!("\"revision-{}\"", document.is_some() as u8 + 1);
    *document = Some((next_etag, String::from_utf8(body.to_vec()).unwrap()));
    StatusCode::NO_CONTENT
}

fn authorized(headers: &HeaderMap) -> bool {
    let expected = format!("Basic {}", STANDARD.encode("sync-user:sync-password"));
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        == Some(expected.as_str())
}

#[tokio::test]
async fn two_devices_sync_encrypted_records_through_webdav() {
    let state = WebDavState::default();
    let app = Router::new()
        .route(
            "/vibeshell-sync.json",
            get(read_document).put(write_document),
        )
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let endpoint = format!("http://{address}/vibeshell-sync.json");

    let directory = tempfile::tempdir().unwrap();
    let source = Arc::new(Database::new_at(directory.path().join("source.db")).unwrap());
    let target = Arc::new(Database::new_at(directory.path().join("target.db")).unwrap());
    let source_sync = CloudSyncManager::new(source.clone()).unwrap();
    let target_sync = CloudSyncManager::new(target.clone()).unwrap();
    let pairing = source_sync
        .create_webdav_vault(
            endpoint,
            "sync-user".to_string(),
            "sync-password".to_string(),
        )
        .await
        .unwrap();
    target_sync.join_vault(&pairing.pairing_code).await.unwrap();

    let mut source_server = Server {
        id: String::new(),
        name: "webdav-production".to_string(),
        host: "webdav-secret.internal".to_string(),
        port: 22,
        username: "deploy".to_string(),
        auth_type: AuthType::Password,
        credential_id: Some("device-only-credential".to_string()),
        group_id: None,
        tags: Vec::new(),
        created_at: 0,
        updated_at: 0,
        jump_host_id: None,
        post_login_command: None,
        agent_forwarding: false,
    };
    source.server_add(&mut source_server).unwrap();

    assert_eq!(source_sync.sync_now().await.unwrap().uploaded, 1);
    assert_eq!(target_sync.sync_now().await.unwrap().applied, 1);
    let restored = target.server_get(&source_server.id).unwrap().unwrap();
    assert_eq!(restored.host, "webdav-secret.internal");
    assert!(restored.credential_id.is_none());

    let remote_document = state.document.lock().unwrap().clone().unwrap().1;
    assert!(!remote_document.contains("webdav-secret.internal"));
    assert!(!remote_document.contains("device-only-credential"));
    server.abort();
}
