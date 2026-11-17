//! Opt-in, pinned-host OpenSSH regression suite. Run scripts/test-ssh-compatibility.sh.
//! No test uses the user's server database, SSH keys, or trust store.
use anyhow::{Context, Result};
use std::{env, fs, path::PathBuf, sync::Arc, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use vibeshell_core::ssh::{FingerprintStore, HostKeyCheck, PtyConfig, SshClient};
use vibeshell_core::storage::models::{TunnelConfig, TunnelStatus, TunnelType};
use vibeshell_core::tunnel::TunnelManager;

fn required(name: &str) -> String {
    env::var(name)
        .unwrap_or_else(|_| panic!("{name} missing; run scripts/test-ssh-compatibility.sh"))
}
fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(required("SSH_TEST_FIXTURE_DIR")).join(name)
}

struct Fixture {
    client: SshClient,
    output: mpsc::Receiver<Vec<u8>>,
    store: Arc<FingerprintStore>,
    _directory: tempfile::TempDir,
    host: String,
    port: u16,
}
impl Fixture {
    fn new(port_name: &str, key_name: &str) -> Result<Self> {
        let directory = tempfile::tempdir()?;
        let store = Arc::new(FingerprintStore::new_at(
            directory.path().join("trust.json"),
        )?);
        let host = required("SSH_TEST_HOST");
        let port = required(port_name).parse()?;
        let key =
            russh::keys::PublicKey::from_openssh(&fs::read_to_string(fixture_path(key_name))?)?;
        store.save(
            &host,
            port,
            &key.fingerprint(russh::keys::HashAlg::Sha256).to_string(),
            key.algorithm().as_ref(),
            None,
        )?;
        let (sender, output) = mpsc::channel(64);
        let mut client = SshClient::new(sender);
        client.set_host_key_check(HostKeyCheck::new(store.clone(), &host, port));
        Ok(Self {
            client,
            output,
            store,
            _directory: directory,
            host,
            port,
        })
    }
    async fn password(&mut self) -> Result<()> {
        self.client
            .connect_password(
                &self.host,
                self.port,
                &required("SSH_TEST_USER"),
                &required("SSH_TEST_PASSWORD"),
            )
            .await
    }
    async fn key(&mut self, name: &str, passphrase: Option<&str>) -> Result<()> {
        self.client
            .connect_key(
                &self.host,
                self.port,
                &required("SSH_TEST_USER"),
                &fs::read_to_string(fixture_path(name))?,
                passphrase,
            )
            .await
    }
}

#[tokio::test]
#[ignore = "isolated OpenSSH fixture required"]
async fn authentication_matrix() -> Result<()> {
    for (port, host_key) in [
        ("SSH_TEST_PORT", "host.pub"),
        ("SSH_TEST_PAM_PORT", "host.pub"),
        ("SSH_TEST_COMPAT_PORT", "host-rsa.pub"),
    ] {
        let mut fixture = Fixture::new(port, host_key)?;
        fixture
            .password()
            .await
            .with_context(|| format!("Password/PAM policy {port}"))?;
        assert_eq!(
            fixture.client.exec_command("whoami").await?.trim(),
            required("SSH_TEST_USER")
        );
        fixture.client.disconnect().await?;
        for (name, passphrase) in [
            ("ed25519", None),
            ("rsa", None),
            ("ecdsa", None),
            ("encrypted", Some("fixture-key-passphrase")),
            ("rsa-pem", None),
        ] {
            let mut fixture = Fixture::new(port, host_key)?;
            fixture
                .key(name, passphrase)
                .await
                .with_context(|| format!("Key {name}, policy {port}"))?;
            assert_eq!(
                fixture.client.exec_command("printf key-ok").await?,
                "key-ok"
            );
            fixture.client.disconnect().await?;
            println!("PASS authentication: {port} / {name}");
        }
    }
    Ok(())
}

#[tokio::test]
#[ignore = "isolated OpenSSH fixture required"]
async fn unknown_changed_and_wrong_credentials_fail_closed() -> Result<()> {
    let mut fixture = Fixture::new("SSH_TEST_PORT", "host.pub")?;
    fixture.store.clear()?;
    assert!(fixture.password().await.is_err());
    assert!(!fixture.client.is_connected().await);
    fixture.store.save(
        &fixture.host,
        fixture.port,
        "SHA256:not-the-server-key",
        "ssh-ed25519",
        None,
    )?;
    assert!(fixture.password().await.is_err());
    assert!(!fixture.client.is_connected().await);
    let mut fixture = Fixture::new("SSH_TEST_PORT", "host.pub")?;
    assert!(fixture
        .client
        .connect_password(
            &fixture.host,
            fixture.port,
            &required("SSH_TEST_USER"),
            "intentionally-wrong"
        )
        .await
        .is_err());
    assert!(!fixture.client.is_connected().await);
    assert!(fixture
        .key("encrypted", Some("wrong-passphrase"))
        .await
        .is_err());
    assert!(!fixture.client.is_connected().await);
    Ok(())
}

#[tokio::test]
#[ignore = "isolated OpenSSH fixture required"]
async fn exec_eof_unicode_exit_status_and_channel_reuse() -> Result<()> {
    let mut fixture = Fixture::new("SSH_TEST_PORT", "host.pub")?;
    fixture.password().await?;
    assert_eq!(
        tokio::time::timeout(
            Duration::from_secs(5),
            fixture.client.exec_command("cat; printf eof-ok")
        )
        .await??,
        "eof-ok"
    );
    assert_eq!(
        fixture
            .client
            .exec_command_with_stdin("cat", Some("中文 input"))
            .await?,
        "中文 input\n"
    );
    assert_eq!(
        fixture
            .client
            .exec_command("python3 -c \"print('中文測試'*50,end='')\"")
            .await?,
        "中文測試".repeat(50)
    );
    for index in 0..24 {
        assert_eq!(
            fixture
                .client
                .exec_command(&format!("printf run-{index}"))
                .await?,
            format!("run-{index}")
        );
    }
    let result = fixture
        .client
        .exec_command_result("printf failure; exit 7", None)
        .await?;
    assert_eq!(result.output, "failure");
    assert_eq!(result.exit_code, 7);
    // EOF is not process completion: do not mistake a still-running job for success.
    let result = fixture
        .client
        .exec_command_result("exec 1>&- 2>&-; sleep 3; exit 9", None)
        .await?;
    assert_eq!(result.exit_code, 9);
    fixture.client.disconnect().await?;
    Ok(())
}

#[tokio::test]
#[ignore = "isolated OpenSSH fixture required"]
async fn excessive_exec_output_is_bounded_and_connection_survives() -> Result<()> {
    let mut fixture = Fixture::new("SSH_TEST_PORT", "host.pub")?;
    fixture.password().await?;
    let error = fixture
        .client
        .exec_command("python3 -c \"print('x'*9000000)\"")
        .await
        .unwrap_err();
    assert!(format!("{error:#}").contains("8 MiB"));
    assert_eq!(fixture.client.exec_command("printf alive").await?, "alive");
    fixture.client.disconnect().await?;
    Ok(())
}

#[tokio::test]
#[ignore = "isolated OpenSSH fixture required"]
async fn pty_unicode_resize_large_output_and_exit() -> Result<()> {
    let mut fixture = Fixture::new("SSH_TEST_PORT", "host.pub")?;
    fixture.password().await?;
    fixture
        .client
        .open_shell(Some(PtyConfig {
            cols: 101,
            rows: 37,
            ..Default::default()
        }))
        .await?;
    fixture.client.resize_pty(113, 41).await?;
    fixture.client.send_data(b"stty -echo; stty size; python3 -c \"print('x'*262144)\"; printf '\\nPTY_DONE\\n'; exit\n").await?;
    let output = tokio::time::timeout(Duration::from_secs(15), async {
        let mut result = Vec::new();
        while let Some(chunk) = fixture.output.recv().await {
            result.extend(chunk);
            if result
                .windows(b"PTY_DONE\r\n".len())
                .any(|part| part == b"PTY_DONE\r\n")
            {
                break;
            }
        }
        result
    })
    .await?;
    let text = String::from_utf8_lossy(&output);
    assert!(text.contains("41 113"));
    assert!(output.iter().filter(|&&byte| byte == b'x').count() >= 262144);
    tokio::time::timeout(Duration::from_secs(5), async {
        while fixture.client.is_shell_open().await {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await?;
    assert!(fixture.client.is_connected().await);
    fixture.client.open_shell(None).await?;
    assert!(fixture.client.is_shell_open().await);
    fixture.client.disconnect().await?;
    Ok(())
}

#[tokio::test]
#[ignore = "isolated OpenSSH fixture required"]
async fn sftp_binary_unicode_roundtrip_and_reuse() -> Result<()> {
    let mut fixture = Fixture::new("SSH_TEST_PORT", "host.pub")?;
    fixture.password().await?;
    let data: Vec<u8> = (0..1_048_576).map(|index| (index % 251) as u8).collect();
    for index in 0..10 {
        let sftp = fixture.client.open_sftp_session().await?;
        let path = format!("/home/audit/中文 文件-{index}.bin");
        let mut file = sftp.create(&path).await?;
        file.write_all(&data).await?;
        file.shutdown().await?;
        drop(file);
        let mut file = sftp.open(&path).await?;
        let mut actual = Vec::new();
        file.read_to_end(&mut actual).await?;
        assert_eq!(actual, data);
        drop(file);
        sftp.remove_file(&path).await?;
        sftp.close().await?;
    }
    assert_eq!(
        fixture.client.exec_command("printf after-sftp").await?,
        "after-sftp"
    );
    fixture.client.disconnect().await?;
    Ok(())
}

#[tokio::test]
#[ignore = "isolated OpenSSH fixture required"]
async fn streaming_download_and_same_size_directory_sync() -> Result<()> {
    use vibeshell_core::sftp::sync::download_remote_file_streaming;
    use vibeshell_core::sftp::{
        transfer_directory_to_sftp, DirectoryTransferMode, DirectoryTransferOptions,
    };
    let mut fixture = Fixture::new("SSH_TEST_PORT", "host.pub")?;
    fixture.password().await?;
    let sftp = fixture.client.open_sftp_session().await?;
    let source = tempfile::tempdir()?;
    let destination = tempfile::tempdir()?;
    let payload: Vec<u8> = (0..600_123).map(|index| (index % 251) as u8).collect();
    fs::write(source.path().join("binary.bin"), &payload)?;
    fs::write(source.path().join("same.txt"), b"old!")?;
    let remote = format!("/home/audit/helper-{}", uuid::Uuid::new_v4());
    let options = DirectoryTransferOptions {
        excluded_paths: vec![],
        respect_gitignore: false,
        delete_extra: false,
    };
    let first = transfer_directory_to_sftp(
        &sftp,
        source.path(),
        &remote,
        DirectoryTransferMode::Sync,
        &options,
    )
    .await
    .map_err(anyhow::Error::msg)?;
    assert_eq!(first.uploaded_files, 2);
    fs::write(source.path().join("same.txt"), b"new!")?;
    let second = transfer_directory_to_sftp(
        &sftp,
        source.path(),
        &remote,
        DirectoryTransferMode::Sync,
        &options,
    )
    .await
    .map_err(anyhow::Error::msg)?;
    assert_eq!(second.uploaded_files, 1);
    assert_eq!(second.skipped_files, 1);
    for (name, expected) in [
        ("binary.bin", payload.as_slice()),
        ("same.txt", b"new!".as_slice()),
    ] {
        let path = destination.path().join(name);
        let count = download_remote_file_streaming(&sftp, &format!("{remote}/{name}"), &path)
            .await
            .map_err(anyhow::Error::msg)?;
        assert_eq!(count, expected.len() as u64);
        let actual = fs::read(path)?;
        assert_eq!(actual.len(), expected.len());
        assert!(actual == expected, "streamed file differs: {name}");
    }
    sftp.close().await?;
    fixture.client.disconnect().await?;
    Ok(())
}

fn config(kind: TunnelType) -> TunnelConfig {
    TunnelConfig {
        id: "fixture-forward".into(),
        server_id: "fixture".into(),
        tunnel_type: kind,
        local_host: "127.0.0.1".into(),
        local_port: 0,
        remote_host: Some("127.0.0.1".into()),
        remote_port: Some(22),
        auto_start: false,
        enabled: true,
    }
}

#[tokio::test]
#[ignore = "isolated OpenSSH fixture required"]
async fn local_and_socks_forwarding_readiness_duplicates_and_stop() -> Result<()> {
    let mut fixture = Fixture::new("SSH_TEST_PORT", "host.pub")?;
    fixture.password().await?;
    let manager = TunnelManager::new();
    for kind in [TunnelType::Local, TunnelType::Dynamic] {
        let socks = kind == TunnelType::Dynamic;
        let setup = config(kind);
        let tunnel = manager
            .create_tunnel("fixture", fixture.client.clone(), setup.clone())
            .await?;
        assert_eq!(tunnel.status, TunnelStatus::Active);
        assert_ne!(tunnel.config.local_port, 0);
        assert!(manager
            .create_tunnel("fixture", fixture.client.clone(), setup)
            .await
            .is_err());
        let mut stream = TcpStream::connect(("127.0.0.1", tunnel.config.local_port)).await?;
        if socks {
            stream.write_all(&[5, 1, 0]).await?;
            let mut greeting = [0; 2];
            stream.read_exact(&mut greeting).await?;
            assert_eq!(greeting, [5, 0]);
            stream.write_all(&[5, 1, 0, 1, 127, 0, 0, 1, 0, 22]).await?;
            let mut response = [0; 10];
            stream.read_exact(&mut response).await?;
            assert_eq!(response[1], 0);
        }
        let mut banner = [0; 256];
        let count =
            tokio::time::timeout(Duration::from_secs(5), stream.read(&mut banner)).await??;
        assert!(std::str::from_utf8(&banner[..count])?.starts_with("SSH-2.0-OpenSSH"));
        manager.stop_tunnel(&tunnel.id).await?;
        assert!(TcpStream::connect(("127.0.0.1", tunnel.config.local_port))
            .await
            .is_err());
        let closed = tokio::time::timeout(Duration::from_secs(3), stream.read(&mut banner)).await?;
        assert!(matches!(closed, Ok(0) | Err(_)));
    }
    let occupied = TcpListener::bind(("127.0.0.1", 0)).await?;
    let mut setup = config(TunnelType::Local);
    setup.local_port = occupied.local_addr()?.port();
    assert!(manager
        .create_tunnel("fixture", fixture.client.clone(), setup)
        .await
        .is_err());
    assert!(manager.list_tunnels(None).await.is_empty());
    fixture.client.disconnect().await?;
    Ok(())
}

#[tokio::test]
#[ignore = "isolated OpenSSH fixture required"]
async fn reverse_forwarding_half_close_and_remote_cancellation() -> Result<()> {
    let mut fixture = Fixture::new("SSH_TEST_PORT", "host.pub")?;
    fixture.password().await?;
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let mut setup = config(TunnelType::Remote);
    setup.local_port = listener.local_addr()?.port();
    setup.remote_port = Some(0);
    let manager = TunnelManager::new();
    let tunnel = manager
        .create_tunnel("fixture", fixture.client.clone(), setup)
        .await?;
    let port = tunnel.config.remote_port.unwrap();
    assert_ne!(port, 0);
    let response = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        stream.read_to_end(&mut request).await.unwrap();
        assert_eq!(request, b"request");
        stream.write_all(&vec![b'x'; 65536]).await.unwrap();
        stream.shutdown().await.unwrap();
    });
    let command = format!("python3 -c \"import socket; s=socket.create_connection(('127.0.0.1',{port}),5); s.sendall(b'request'); s.shutdown(socket.SHUT_WR); f=s.makefile('rb'); d=f.read(); assert d==b'x'*65536; print(len(d))\"");
    assert_eq!(
        tokio::time::timeout(
            Duration::from_secs(15),
            fixture.client.exec_command(&command)
        )
        .await??
        .trim(),
        "65536"
    );
    response.await?;
    manager.stop_tunnel(&tunnel.id).await?;
    let check = format!("python3 -c \"import socket; s=socket.socket(); s.settimeout(3); assert s.connect_ex(('127.0.0.1',{port}))!=0; print('closed')\"");
    assert_eq!(fixture.client.exec_command(&check).await?.trim(), "closed");
    fixture.client.disconnect().await?;
    Ok(())
}

#[tokio::test]
#[ignore = "isolated OpenSSH fixture required"]
async fn jump_bridge_checks_target_identity_and_preserves_transport() -> Result<()> {
    let mut jump = Fixture::new("SSH_TEST_PORT", "host.pub")?;
    jump.password().await?;
    let manager = TunnelManager::new();
    let tunnel = manager
        .create_tunnel("jump", jump.client.clone(), config(TunnelType::Local))
        .await?;
    let mut target = Fixture::new("SSH_TEST_PORT", "host.pub")?;
    target.client.set_host_key_check(HostKeyCheck::new(
        target.store.clone(),
        "target.internal",
        22,
    ));
    let error = target
        .client
        .connect_password(
            "127.0.0.1",
            tunnel.config.local_port,
            &required("SSH_TEST_USER"),
            &required("SSH_TEST_PASSWORD"),
        )
        .await
        .unwrap_err();
    assert!(format!("{error:#}").contains("target.internal"));
    let public =
        russh::keys::PublicKey::from_openssh(&fs::read_to_string(fixture_path("host.pub"))?)?;
    target.store.save(
        "target.internal",
        22,
        &public.fingerprint(russh::keys::HashAlg::Sha256).to_string(),
        public.algorithm().as_ref(),
        None,
    )?;
    target
        .client
        .connect_password(
            "127.0.0.1",
            tunnel.config.local_port,
            &required("SSH_TEST_USER"),
            &required("SSH_TEST_PASSWORD"),
        )
        .await?;
    assert_eq!(
        target.client.exec_command("printf through-jump").await?,
        "through-jump"
    );
    target.client.disconnect().await?;
    manager.stop_all().await;
    jump.client.disconnect().await?;
    Ok(())
}
