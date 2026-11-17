//! Opt-in: `scripts/test-ssh-compatibility.sh` supplies a loopback sshd, a
//! pinned host public key, and a freshly built CLI. No saved servers are used.
use super::*;
use crate::{ssh::FingerprintStore, storage::models::Server};
use std::{env, process::Command, time::Duration};

#[test]
#[ignore = "requires the isolated OpenSSH and CLI fixture"]
fn cli_and_desktop_share_one_shell_and_one_attributed_history() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let host = env::var("SSH_TEST_HOST")?;
    anyhow::ensure!(host == "127.0.0.1" && socket_name_base().starts_with("test-shared-"));
    let port: u16 = env::var("SSH_TEST_PORT")?.parse()?;
    let username = env::var("SSH_TEST_USER")?;
    let password = env::var("SSH_TEST_PASSWORD")?;
    let db = Arc::new(Database::new_at(temp.path().join("test.db"))?);
    let mut config: Server = serde_json::from_value(
        serde_json::json!({"id":"", "name":"fixture", "host":host, "port":port, "username":username, "auth_type":"password", "tags":[], "created_at":0, "updated_at":0}),
    )?;
    db.server_add(&mut config)?;
    db.credential_save("fixture", "password", &password, None, None)?;
    let manager = Arc::new(SessionManager::new(db.clone()));
    let store = Arc::new(FingerprintStore::new_at(temp.path().join("trust.json"))?);
    let key_file = PathBuf::from(env::var("SSH_TEST_FIXTURE_DIR")?).join("host.pub");
    let key = russh::keys::PublicKey::from_openssh(&std::fs::read_to_string(key_file)?)?;
    store.save(
        &host,
        port,
        &key.fingerprint(russh::keys::HashAlg::Sha256).to_string(),
        key.algorithm().as_ref(),
        None,
    )?;
    manager.set_fingerprint_store(store);
    let service = IpcServer::new(db.clone(), manager);
    std::thread::spawn(move || service.run().expect("isolated IPC listener"));
    for _ in 0..100 {
        if IpcClient::send(&IpcMessage::ListSessions).is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(matches!(
        IpcClient::send(&IpcMessage::ListSessions)?,
        IpcMessage::SessionList { .. }
    ));
    let cli = |args: &[&str]| -> Result<String> {
        let result = Command::new(env::var("VIBESHELL_TEST_CLI")?)
            .args(args)
            .env("VIBESHELL_SKIP_SKILL_AUTO_INSTALL", "1")
            .env("VIBESHELL_SESSION_STORE", temp.path().join("aliases.json"))
            .output()?;
        anyhow::ensure!(
            result.status.success(),
            "CLI failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        Ok(String::from_utf8(result.stdout)?)
    };
    let agent_cmd = "export VS_TEST_AGENT=from_cli";
    cli(&["ssh", "fixture", "--", agent_cmd])?;
    let IpcMessage::SessionList { sessions } = IpcClient::send(&IpcMessage::ListSessions)? else {
        panic!("missing list")
    };
    assert_eq!(
        sessions.len(),
        1,
        "one-shot ssh must leave the shell available"
    );
    let id = &sessions[0].id;
    // The desktop attaches via this unmarked stream when the daemon owns the shell.
    let mut desktop = IpcClient::connect_streaming(&IpcMessage::AttachSession {
        session_id: id.clone(),
    })?;
    let mut ack = String::new();
    desktop.read_line(&mut ack)?;
    assert!(matches!(serde_json::from_str(&ack)?, IpcMessage::Ok));
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in desktop.lines().map_while(Result::ok) {
            if let Ok(msg) = serde_json::from_str::<IpcMessage>(&line) {
                if tx.send(msg).is_err() {
                    break;
                }
            }
        }
    });
    let human_cmd = "export VS_TEST_USER=from_ui";
    assert!(matches!(
        IpcClient::send(&IpcMessage::SendUserInput {
            session_id: id.clone(),
            data: format!("{human_cmd}\r").into_bytes()
        })?,
        IpcMessage::Ok
    ));
    let check = r#"printf 'result:%s/%s\n' "$VS_TEST_AGENT" "$VS_TEST_USER""#;
    let output = cli(&["exec", id, "--", check])?;
    assert!(
        output.contains("result:from_cli/from_ui"),
        "isolated shell or missing output: {output:?}"
    );
    let until = std::time::Instant::now() + Duration::from_secs(5);
    let mut live = Vec::new();
    let mut decorated = false;
    while !String::from_utf8_lossy(&live).contains("result:from_cli/from_ui") || !decorated {
        match rx.recv_timeout(until.saturating_duration_since(std::time::Instant::now()))? {
            IpcMessage::SessionOutput { data, .. } => live.extend(data),
            IpcMessage::CliTyping { session_id, text } => {
                assert_eq!(session_id, *id);
                assert!(text.chars().all(char::is_whitespace));
                decorated = true;
            }
            _ => {}
        }
    }
    let rows = db.activity_page(None, None, 100, true)?;
    for (origin, command) in [("cli", agent_cmd), ("user", human_cmd), ("cli", check)] {
        assert_eq!(
            rows.iter()
                .filter(|r| r.origin.as_deref() == Some(origin)
                    && r.event.session_id.as_ref() == Some(id)
                    && r.event.summary == command
                    && r.event.status == crate::mcp::server::AgentActivityStatus::Succeeded)
                .count(),
            1,
            "incorrect history for {command}: {:?}",
            rows.iter()
                .map(|r| (&r.origin, &r.event.summary))
                .collect::<Vec<_>>()
        );
    }
    assert!(rows
        .iter()
        .any(|r| r.event.tool == "cli.session_create" && r.event.session_id.as_ref() == Some(id)));
    assert!(!rows.iter().any(|r| r.event.summary.contains(&password)));
    assert!(matches!(
        IpcClient::send(&IpcMessage::KillSession {
            session_id: id.clone()
        })?,
        IpcMessage::Ok
    ));
    Ok(())
}
