use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use vibeshell_core::{Database, IpcServer, IpcServerRunError, SessionManager};

fn unique_endpoint_name() -> String {
    format!(
        "vibeshell-race-test-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos()
    )
}

struct EndpointEnvGuard {
    previous: Option<String>,
}

impl EndpointEnvGuard {
    const SOCKET_NAME_ENV: &'static str = "VIBESHELL_IPC_NAME";

    fn set(endpoint: &str) -> Self {
        let previous = std::env::var(Self::SOCKET_NAME_ENV).ok();
        std::env::set_var(Self::SOCKET_NAME_ENV, endpoint);
        Self { previous }
    }
}

impl Drop for EndpointEnvGuard {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(value) => std::env::set_var(Self::SOCKET_NAME_ENV, value),
            None => std::env::remove_var(Self::SOCKET_NAME_ENV),
        }
    }
}

#[test]
fn competing_processes_on_same_endpoint_have_single_winner_without_split_brain() {
    let _guard = EndpointEnvGuard::set(&unique_endpoint_name());

    let database = Arc::new(Database::new().expect("database should initialize"));
    let session_manager = Arc::new(SessionManager::new(database.clone()));

    let (winner_tx, winner_rx) = mpsc::channel();
    {
        let db = database.clone();
        let sm = session_manager.clone();
        thread::spawn(move || {
            let server = IpcServer::new(db, sm);
            let result = server.run();
            let _ = winner_tx.send(result);
        });
    }

    let early_result = winner_rx.recv_timeout(Duration::from_millis(200));

    if let Ok(result) = early_result {
        panic!(
            "first contender exited unexpectedly before contention check: {}",
            result
                .err()
                .map(|err| err.to_string())
                .unwrap_or_else(|| "Ok(())".to_string())
        );
    }

    let loser = IpcServer::new(database, session_manager)
        .run()
        .expect_err("second contender should fail to bind endpoint");

    assert!(
        matches!(loser, IpcServerRunError::ListenerBind(_)),
        "second contender must fail with listener bind error"
    );
}
