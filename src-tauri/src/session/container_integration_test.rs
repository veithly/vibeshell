//! Opt-in real Docker-in-Docker fixture. No production data or host Docker
//! socket is accessible. Run scripts/test-container-sessions.sh.
use crate as vibeshell_core;
use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};
use std::{env, fs, path::PathBuf, process::Command, sync::Arc, time::Duration};
use vibeshell_core::{
    ipc::{IpcClient, IpcMessage},
    mcp::server::{execute_tool_public, McpState},
    session::{
        container::ContainerOpenRequest, Session, SessionManager, SessionState, SshCredential,
    },
    ssh::{FingerprintStore, PtyConfig},
    storage::{Database, PluginInstallation, Server},
    IpcServer,
};

fn required(key: &str) -> Result<String> {
    env::var(key).with_context(|| format!("{key} missing; use scripts/test-container-sessions.sh"))
}
fn request(parent: &str, container: &str) -> ContainerOpenRequest {
    ContainerOpenRequest {
        parent_session_id: parent.into(),
        container: container.into(),
        user: None,
        workdir: Some("/tmp".into()),
        shell: "/bin/sh".into(),
    }
}
async fn wait_output(session: &Session, needle: &str) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            if String::from_utf8_lossy(&session.replay_output().await.concat()).contains(needle) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .context("Terminal output did not arrive")?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires an isolated nested Docker engine and pinned SSH fixture"]
async fn real_containers_keep_terminal_exec_plugins_cli_and_mcp_in_one_scope() -> Result<()> {
    let host = required("SSH_TEST_HOST")?;
    anyhow::ensure!(
        host == "127.0.0.1" && required("VIBESHELL_IPC_NAME")?.starts_with("test-containers-")
    );
    let port = required("SSH_TEST_PORT")?.parse()?;
    let alpha = required("VIBESHELL_CONTAINER_A")?;
    let beta = required("VIBESHELL_CONTAINER_B")?;
    let stopped = required("VIBESHELL_CONTAINER_STOPPED")?;
    let temp = tempfile::tempdir()?;
    let db = Arc::new(Database::new_at(temp.path().join("fixture.db"))?);
    let mut profile: Server = serde_json::from_value(
        json!({"id":"", "name":"isolated-host", "host":host, "port":port,
        "username":required("SSH_TEST_USER")?, "auth_type":"password", "tags":[], "created_at":0, "updated_at":0}),
    )?;
    db.server_add(&mut profile)?;
    // Exercise the actual SSH host-key/authentication path through CONNECT,
    // not only a protocol stub. This process runs one explicitly selected test.
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let proxy = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
    let proxy_address = proxy.local_addr()?;
    vibeshell_core::network::ProxySettings::save(
        &db,
        vibeshell_core::network::ProxySettingsInput {
            mode: vibeshell_core::network::ProxyMode::Custom,
            url: format!("http://{proxy_address}"),
            username: String::new(),
            password: None,
        },
    )?;
    vibeshell_core::network::initialize(db.clone());
    let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let proxy_count = count.clone();
    let proxy_task = tokio::spawn(async move {
        while let Ok((mut socket, _)) = proxy.accept().await {
            let count = proxy_count.clone();
            tokio::spawn(async move {
                let mut header = vec![];
                while !header.ends_with(b"\r\n\r\n") {
                    header.push(socket.read_u8().await.unwrap());
                    assert!(header.len() < 16384);
                }
                assert!(String::from_utf8_lossy(&header)
                    .starts_with(&format!("CONNECT 127.0.0.1:{port} HTTP/1.1")));
                let mut target = tokio::net::TcpStream::connect(("127.0.0.1", port))
                    .await
                    .unwrap();
                socket
                    .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
                    .await
                    .unwrap();
                count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let _ = tokio::io::copy_bidirectional(&mut socket, &mut target).await;
            });
        }
    });
    let manager = Arc::new(SessionManager::new(db.clone()));
    let trust = Arc::new(FingerprintStore::new_at(temp.path().join("trust.json"))?);
    let key = russh::keys::PublicKey::from_openssh(&fs::read_to_string(
        PathBuf::from(required("SSH_TEST_FIXTURE_DIR")?).join("host.pub"),
    )?)?;
    trust.save(
        &host,
        port,
        &key.fingerprint(russh::keys::HashAlg::Sha256).to_string(),
        key.algorithm().as_ref(),
        None,
    )?;
    manager.set_fingerprint_store(trust);
    let parent = manager
        .create_with_credentials(
            "isolated-host",
            SshCredential::Password(required("SSH_TEST_PASSWORD")?),
            Some(PtyConfig::default()),
        )
        .await?;
    let inventory = manager.list_containers(&parent.id).await?;
    assert_eq!(
        count.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "SSH authentication bypassed the configured proxy"
    );
    assert!(inventory
        .iter()
        .any(|c| c.id == alpha && c.state == "running"));
    assert!(inventory
        .iter()
        .any(|c| c.id == stopped && c.state != "running"));
    assert!(manager
        .create_container(request(&parent.id, &stopped))
        .await
        .is_err());
    let a = manager
        .create_container(request(&parent.id, &alpha))
        .await?;
    let b = manager.create_container(request(&parent.id, &beta)).await?;
    assert_ne!(a.id, b.id);
    assert_eq!(a.container.as_ref().unwrap().container_id, alpha);
    assert_eq!(
        a.exec_quick_command("hostname").await?.output.trim(),
        "fixture-alpha"
    );
    assert_eq!(
        b.exec_quick_command("hostname").await?.output.trim(),
        "fixture-beta"
    );
    assert_eq!(a.exec_quick_command("pwd").await?.output.trim(), "/tmp");
    assert_eq!(a.exec_quick_command("exit 7").await?.exit_code, 7);
    assert!(a.open_sftp_session().await.is_err());
    assert!(a.get_ssh_client().await.is_none());
    assert!(manager
        .connect_session(&a.id, SshCredential::Password(String::new()), None)
        .await
        .is_err());
    a.write_to_ssh(b"cd /; export FIXTURE_SCOPE=alpha\r")
        .await?;
    a.write_to_ssh(b"printf 'persistent:%s:%s\\n' \"$FIXTURE_SCOPE\" \"$PWD\"\r")
        .await?;
    wait_output(&a, "persistent:alpha:/").await?;

    // A declarative plugin must execute inside the container, not on its host.
    let manifest = json!({"schemaVersion":1,"id":"fixture-hostname","name":"Fixture hostname","description":"Read test container hostname","version":"1.0.0","author":"VibeShell tests","category":"operations","icon":"terminal","permissions":["remote_exec"],"sessionTypes":["ssh"],"entry":{"type":"commands","actions":[{"id":"read","name":"Read hostname","description":"Read the current execution namespace hostname","program":"hostname","args":[]}]}});
    db.plugin_installation_upsert(&PluginInstallation {
        plugin_id: "fixture-hostname".into(),
        version: "1.0.0".into(),
        manifest_json: manifest.to_string(),
        source: "external".into(),
        enabled: true,
        granted_permissions_json: r#"["remote_exec"]"#.into(),
        settings_json: "{}".into(),
        installed_at: 1,
        updated_at: 1,
    })?;
    let local_mcp = McpState::new(db.clone(), manager.clone());
    assert_eq!(
        execute_tool_public(
            &local_mcp,
            "exec",
            &json!({"session_id":a.id,"command":"hostname"})
        )
        .await
        .map_err(anyhow::Error::msg)?
        .trim(),
        "fixture-alpha"
    );
    let plugin = execute_tool_public(
        &local_mcp,
        "plugin_execute",
        &json!({"sessionId":a.id,"pluginId":"fixture-hostname","actionId":"read","inputs":{}}),
    )
    .await
    .map_err(anyhow::Error::msg)?;
    assert!(plugin.contains("fixture-alpha"));

    let service = IpcServer::new(db.clone(), manager.clone());
    std::thread::spawn(move || service.run().expect("isolated container IPC"));
    for _ in 0..100 {
        if tokio::task::spawn_blocking(|| IpcClient::send(&IpcMessage::ListSessions))
            .await?
            .is_ok()
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    // The GUI Gateway can be a different process/manager than the daemon.
    let remote_mcp =
        McpState::new(db.clone(), Arc::new(SessionManager::new(db.clone()))).with_owner_routing();
    let listed = execute_tool_public(&remote_mcp, "session_list", &json!({}))
        .await
        .map_err(anyhow::Error::msg)?;
    assert!(listed.contains(&alpha));
    assert_eq!(
        execute_tool_public(
            &remote_mcp,
            "exec",
            &json!({"session_id":b.id,"command":"hostname"})
        )
        .await
        .map_err(anyhow::Error::msg)?
        .trim(),
        "fixture-beta"
    );
    let remote_plugin = execute_tool_public(
        &remote_mcp,
        "plugin_execute",
        &json!({"sessionId":b.id,"pluginId":"fixture-hostname","actionId":"read","inputs":{}}),
    )
    .await
    .map_err(anyhow::Error::msg)?;
    assert!(remote_plugin.contains("fixture-beta"));
    // Owner forwarding must preserve split-input guard state across calls.
    db.set_setting("agent_guard_config", &json!({"enabled":true,"requireForExec":true,"customPatterns":["touch /tmp/guard-fixture"],"allowPatterns":[]}).to_string())?;
    execute_tool_public(
        &remote_mcp,
        "session_send_input",
        &json!({"session_id":b.id,"data":"touch /tmp/"}),
    )
    .await
    .map_err(anyhow::Error::msg)?;
    assert!(execute_tool_public(
        &remote_mcp,
        "session_send_input",
        &json!({"session_id":b.id,"data":"guard-fixture\n"})
    )
    .await
    .is_err());
    b.write_to_ssh(b"\x03").await?;
    assert_eq!(
        b.exec_quick_command("test ! -e /tmp/guard-fixture")
            .await?
            .exit_code,
        0
    );
    db.set_setting(
        "agent_guard_config",
        &serde_json::to_string(&vibeshell_core::mcp::guard::GuardConfig::default())?,
    )?;
    assert!(execute_tool_public(
        &remote_mcp,
        "exec",
        &json!({"session_id":b.id,"command":"rm -rf /unsafe-test-must-not-run"})
    )
    .await
    .is_err());
    let created = execute_tool_public(
        &remote_mcp,
        "container_open",
        &json!({"parent_session_id":parent.id,"container":alpha}),
    )
    .await
    .map_err(anyhow::Error::msg)?;
    let remote_id = serde_json::from_str::<Value>(&created)?["id"]
        .as_str()
        .ok_or_else(|| anyhow!("missing session ID"))?
        .to_owned();
    manager.kill(&remote_id).await?;

    let cli_path = required("VIBESHELL_TEST_CLI")?;
    let aliases = temp.path().join("aliases.json");
    let cli = |args: Vec<String>| {
        let binary = cli_path.clone();
        let aliases = aliases.clone();
        tokio::task::spawn_blocking(move || -> Result<String> {
            let output = Command::new(binary)
                .args(args)
                .env("VIBESHELL_SKIP_SKILL_AUTO_INSTALL", "1")
                .env("VIBESHELL_SESSION_STORE", aliases)
                .output()?;
            anyhow::ensure!(
                output.status.success(),
                "CLI failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            Ok(String::from_utf8(output.stdout)?)
        })
    };
    let listed = cli(vec![
        "containers".into(),
        "list".into(),
        "--session".into(),
        parent.id.clone(),
    ])
    .await??;
    assert!(listed.contains(&alpha));
    let created = cli(vec![
        "containers".into(),
        "open".into(),
        "--session".into(),
        parent.id.clone(),
        alpha.clone(),
    ])
    .await??;
    let id = serde_json::from_str::<Value>(&created)?["id"]
        .as_str()
        .ok_or_else(|| anyhow!("CLI did not return a child ID"))?
        .to_owned();
    let from_cli = manager
        .get(&id)
        .await
        .ok_or_else(|| anyhow!("CLI child was not registered with the shared owner"))?;
    assert_eq!(from_cli.container.as_ref().unwrap().container_id, alpha);
    manager.kill(&id).await?;
    let output = cli(vec![
        "ssh-session".into(),
        a.id.clone(),
        "--".into(),
        "hostname".into(),
    ])
    .await??;
    assert!(output.contains("fixture-alpha"));

    // An in-flight independent command is cancelled when its child is closed.
    let clone = a.clone();
    let execution = tokio::spawn(async move { clone.exec_command("sleep 30").await });
    tokio::time::sleep(Duration::from_millis(150)).await;
    manager.kill(&a.id).await?;
    assert!(tokio::time::timeout(Duration::from_secs(3), execution)
        .await??
        .is_err());
    assert_eq!(parent.get_state().await, SessionState::Connected);
    assert_eq!(
        parent
            .exec_quick_command(&format!(
                "docker inspect --format '{{{{.State.Running}}}}' -- {alpha}"
            ))
            .await?
            .output
            .trim(),
        "true"
    );
    assert_eq!(
        b.exec_quick_command("hostname").await?.output.trim(),
        "fixture-beta"
    );
    parent
        .exec_quick_command(&format!("docker stop --time 1 -- {beta}"))
        .await?;
    tokio::time::timeout(Duration::from_secs(5), async {
        while b.get_state().await == SessionState::Connected {
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await?;
    assert!(b.exec_quick_command("hostname").await.is_err());
    let c = manager
        .create_container(request(&parent.id, &alpha))
        .await?;
    manager.kill(&parent.id).await?;
    assert_ne!(c.get_state().await, SessionState::Connected);
    assert!(c.exec_quick_command("hostname").await.is_err());
    proxy_task.abort();
    let _ = proxy_task.await;
    assert!(
        manager
            .create_with_credentials(
                "isolated-host",
                SshCredential::Password(required("SSH_TEST_PASSWORD")?),
                Some(PtyConfig::default())
            )
            .await
            .is_err(),
        "A failed proxy must not retry direct SSH even when the server and credentials are valid"
    );
    println!("PASS real container PTYs, exec/plugin/CLI/MCP scope, daemon ownership, stopped refusal, independent teardown and parent disconnect");
    Ok(())
}
