use super::*;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

fn route(url: String) -> Route {
    Route::from_settings(&ProxySettings {
        mode: ProxyMode::Custom,
        url,
        ..Default::default()
    })
    .unwrap()
}
async fn listener(scheme: &str) -> (TcpListener, Route) {
    let server = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let route = route(format!("{scheme}://{}", server.local_addr().unwrap()));
    (server, route)
}
async fn headers(s: &mut tokio::net::TcpStream) -> String {
    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") {
        head.push(s.read_u8().await.unwrap());
        assert!(head.len() < 16384);
    }
    String::from_utf8(head).unwrap()
}

#[tokio::test]
async fn connect_uses_proxy_dns_and_never_consumes_tunneled_bytes_as_headers() {
    let (server, route) = listener("http").await;
    let task = tokio::spawn(async move {
        let (mut s, _) = server.accept().await.unwrap();
        assert!(headers(&mut s)
            .await
            .starts_with("CONNECT unresolved.invalid:22 HTTP/1.1"));
        s.write_all(b"HTTP/1.1 200 Connection established\r\n\r\nSSH-ready")
            .await
            .unwrap();
    });
    let mut stream = connect(&route, "unresolved.invalid", 22).await.unwrap();
    let mut bytes = vec![];
    stream.read_to_end(&mut bytes).await.unwrap();
    assert_eq!(bytes, b"SSH-ready");
    task.await.unwrap();
}

#[tokio::test]
async fn refused_proxy_never_contacts_the_destination() {
    let destination = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let (proxy, route) = listener("http").await;
    let task = tokio::spawn(async move {
        let (mut s, _) = proxy.accept().await.unwrap();
        let _ = headers(&mut s).await;
        s.write_all(b"HTTP/1.1 407 Proxy Authentication Required\r\n\r\n")
            .await
            .unwrap();
    });
    assert!(connect(
        &route,
        "127.0.0.1",
        destination.local_addr().unwrap().port()
    )
    .await
    .is_err());
    assert!(
        tokio::time::timeout(Duration::from_millis(100), destination.accept())
            .await
            .is_err()
    );
    task.await.unwrap();
}

#[tokio::test]
async fn socks4a_and_socks5h_preserve_remote_dns_and_protocol_boundaries() {
    for scheme in ["socks4a", "socks5h"] {
        let (server, route) = listener(scheme).await;
        let task = tokio::spawn(async move {
            let (mut s, _) = server.accept().await.unwrap();
            if scheme == "socks4a" {
                let mut prefix = [0; 8];
                s.read_exact(&mut prefix).await.unwrap();
                assert_eq!(prefix, [4, 1, 0, 22, 0, 0, 0, 1]);
                assert_eq!(s.read_u8().await.unwrap(), 0);
                let mut host = Vec::new();
                loop {
                    let b = s.read_u8().await.unwrap();
                    if b == 0 {
                        break;
                    }
                    host.push(b);
                }
                assert_eq!(host, b"not-resolvable.invalid");
                s.write_all(&[0, 90, 0, 22, 0, 0, 0, 1]).await.unwrap();
            } else {
                let mut hello = [0; 3];
                s.read_exact(&mut hello).await.unwrap();
                assert_eq!(hello, [5, 1, 0]);
                s.write_all(&[5, 0]).await.unwrap();
                let mut prefix = [0; 4];
                s.read_exact(&mut prefix).await.unwrap();
                assert_eq!(prefix, [5, 1, 0, 3]);
                let n = s.read_u8().await.unwrap() as usize;
                let mut host = vec![0; n];
                s.read_exact(&mut host).await.unwrap();
                assert_eq!(host, b"not-resolvable.invalid");
                assert_eq!(s.read_u16().await.unwrap(), 22);
                s.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0]).await.unwrap();
            }
            s.write_all(b"through-proxy").await.unwrap();
        });
        let mut stream = connect(&route, "not-resolvable.invalid", 22).await.unwrap();
        let mut result = vec![];
        stream.read_to_end(&mut result).await.unwrap();
        assert_eq!(result, b"through-proxy");
        task.await.unwrap();
    }
}

#[tokio::test]
async fn socks5_authentication_preserves_spaces_and_rejects_downgrade() {
    use base64::Engine;
    let (server, _) = listener("socks5h").await;
    let settings = ProxySettings {
        mode: ProxyMode::Custom,
        url: format!("socks5h://{}", server.local_addr().unwrap()),
        username: "user".into(),
        encrypted_password: Crypto::device()
            .unwrap()
            .encrypt_base64(b" padded ")
            .unwrap(),
    };
    let route = Route::from_settings(&settings).unwrap();
    let task = tokio::spawn(async move {
        let (mut s, _) = server.accept().await.unwrap();
        let mut hello = [0; 3];
        s.read_exact(&mut hello).await.unwrap();
        assert_eq!(hello, [5, 1, 2]);
        s.write_all(&[5, 2]).await.unwrap();
        assert_eq!(s.read_u8().await.unwrap(), 1);
        let n = s.read_u8().await.unwrap();
        let mut u = vec![0; n as usize];
        s.read_exact(&mut u).await.unwrap();
        assert_eq!(u, b"user");
        let n = s.read_u8().await.unwrap();
        let mut p = vec![0; n as usize];
        s.read_exact(&mut p).await.unwrap();
        assert!(p == b" padded ");
        s.write_all(&[1, 1]).await.unwrap();
    });
    let error = match connect(&route, "node.invalid", 22).await {
        Ok(_) => panic!("accepted rejected authentication"),
        Err(e) => e,
    };
    assert!(!error.to_string().contains("padded"));
    task.await.unwrap();
    // The same encrypted credential must be sent to native HTTP proxies too.
    let (proxy, _) = listener("http").await;
    let settings = ProxySettings {
        url: format!("http://{}", proxy.local_addr().unwrap()),
        ..settings
    };
    let route = Route::from_settings(&settings).unwrap();
    let expected = base64::engine::general_purpose::STANDARD.encode(b"user: padded ");
    let task = tokio::spawn(async move {
        let (mut s, _) = proxy.accept().await.unwrap();
        let head = headers(&mut s).await;
        assert!(head.to_ascii_lowercase().contains(&format!(
            "proxy-authorization: basic {}",
            expected.to_ascii_lowercase()
        )));
        s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
            .await
            .unwrap();
    });
    assert_eq!(
        route
            .client()
            .unwrap()
            .get("http://http-target.invalid/")
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap(),
        "ok"
    );
    task.await.unwrap();
}

#[tokio::test]
async fn tls_proxy_rejects_plaintext_peer_and_cancel_drops_socket() {
    let (server, route) = listener("https").await;
    let task = tokio::spawn(async move {
        let (mut s, _) = server.accept().await.unwrap();
        s.write_all(b"not a TLS server").await.unwrap();
    });
    assert!(connect(&route, "node.invalid", 22).await.is_err());
    task.await.unwrap();
    let (server, route) = listener("http").await;
    let task = tokio::spawn(async move {
        let (mut s, _) = server.accept().await.unwrap();
        let _ = headers(&mut s).await;
        let mut b = [0];
        tokio::time::timeout(Duration::from_secs(3), s.read(&mut b))
            .await
            .unwrap()
            .unwrap()
    });
    assert!(tokio::time::timeout(
        Duration::from_millis(100),
        connect(&route, "node.invalid", 22)
    )
    .await
    .is_err());
    assert_eq!(task.await.unwrap(), 0);
}

#[tokio::test]
async fn https_connect_checks_both_certificate_trust_and_hostname() {
    use tokio_rustls::{
        rustls::{
            pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer},
            ServerConfig,
        },
        TlsAcceptor,
    };
    let leaf =
        CertificateDer::from(include_bytes!("../../tests/fixtures/proxy-test-leaf.der").to_vec());
    let root =
        CertificateDer::from(include_bytes!("../../tests/fixtures/proxy-test-ca.der").to_vec());
    for (trusted, hostname, success) in [
        (true, "localhost", true),
        (false, "localhost", false),
        (true, "127.0.0.1", false),
    ] {
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
            include_bytes!("../../tests/fixtures/proxy-test-key.der").to_vec(),
        ));
        let config = ServerConfig::builder_with_provider(Arc::new(
            tokio_rustls::rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![leaf.clone()], key)
        .unwrap();
        let server = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let mut route = route(format!(
            "https://{hostname}:{}",
            server.local_addr().unwrap().port()
        ));
        if trusted {
            route.test_roots.push(root.clone());
        }
        let task = tokio::spawn(async move {
            let (s, _) = server.accept().await.unwrap();
            if let Ok(mut tls) = TlsAcceptor::from(Arc::new(config)).accept(s).await {
                let mut head = vec![];
                while !head.ends_with(b"\r\n\r\n") {
                    head.push(tls.read_u8().await.unwrap());
                }
                assert!(head.starts_with(b"CONNECT unresolved.invalid:22"));
                tls.write_all(b"HTTP/1.1 200 OK\r\n\r\nTLS-proxied")
                    .await
                    .unwrap();
                tls.shutdown().await.unwrap();
            }
        });
        let result = connect(&route, "unresolved.invalid", 22).await;
        assert_eq!(result.is_ok(), success);
        if let Ok(mut s) = result {
            let mut bytes = vec![];
            s.read_to_end(&mut bytes).await.unwrap();
            assert_eq!(bytes, b"TLS-proxied");
        }
        task.await.unwrap();
    }
}
