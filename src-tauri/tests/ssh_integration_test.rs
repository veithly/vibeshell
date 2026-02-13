//! Integration test for SSH functionality
//!
//! This test connects to a real SSH server to verify the SSH client works correctly.

use anyhow::Result;
use std::path::Path;
use std::sync::Arc;

// Configure these environment variables to run integration tests:
//   SSH_TEST_HOST, SSH_TEST_PORT, SSH_TEST_USER, SSH_TEST_KEY_PATH, SSH_TEST_KEY_PASSPHRASE
const TEST_HOST: &str = "127.0.0.1";
const TEST_PORT: u16 = 22;
const TEST_USER: &str = "testuser";
const TEST_KEY_PATH: &str = "";
const TEST_KEY_PASSPHRASE: &str = "";

#[tokio::test]
#[ignore = "requires a real SSH server and key file — run with `cargo test -- --ignored`"]
async fn test_ssh_connection_with_key() -> Result<()> {
    use async_trait::async_trait;
    use russh::*;
    use russh_keys::*;

    println!("Testing SSH connection to {}@{}:{}", TEST_USER, TEST_HOST, TEST_PORT);

    // Verify key file exists
    let key_path = Path::new(TEST_KEY_PATH);
    assert!(key_path.exists(), "SSH key file should exist at {}", TEST_KEY_PATH);

    // Read the private key
    let private_key = std::fs::read_to_string(key_path)?;
    println!("Private key loaded ({} bytes)", private_key.len());

    // Decode the key first to verify it works
    println!("Decoding private key with passphrase...");
    let key_pair = decode_secret_key(&private_key, Some(TEST_KEY_PASSPHRASE))?;
    println!("Key decoded successfully! Key type: {:?}", key_pair.name());

    // Create a simple handler for testing
    struct TestHandler;

    #[async_trait]
    impl client::Handler for TestHandler {
        type Error = anyhow::Error;

        async fn check_server_key(
            &mut self,
            server_public_key: &key::PublicKey,
        ) -> Result<bool, Self::Error> {
            println!("Server public key: {:?}", server_public_key.name());
            Ok(true) // Accept all host keys for testing
        }
    }

    // Connect to the server with extended timeout
    println!("Connecting to {}:{}...", TEST_HOST, TEST_PORT);
    let mut config = client::Config::default();
    config.inactivity_timeout = Some(std::time::Duration::from_secs(60));
    config.keepalive_interval = Some(std::time::Duration::from_secs(10));
    config.keepalive_max = 5;
    let config = Arc::new(config);

    let mut session = match client::connect(config, (TEST_HOST, TEST_PORT), TestHandler).await {
        Ok(s) => {
            println!("TCP connection established!");
            s
        }
        Err(e) => {
            println!("Connection failed: {:?}", e);
            return Err(e.into());
        }
    };

    // Authenticate with the key
    println!("Authenticating as '{}'...", TEST_USER);
    let auth_result = session
        .authenticate_publickey(TEST_USER, Arc::new(key_pair))
        .await?;

    if auth_result {
        println!("Authentication successful!");
    } else {
        println!("Authentication failed!");
        return Err(anyhow::anyhow!("Authentication failed"));
    }

    // Open a channel and execute a command
    println!("Opening channel...");
    let mut channel = session.channel_open_session().await?;
    println!("Channel opened!");

    // Execute a simple command
    println!("Executing 'whoami' command...");
    channel.exec(true, "whoami").await?;

    // Wait for output
    println!("Waiting for output...");
    let mut output = String::new();
    loop {
        tokio::select! {
            msg = channel.wait() => {
                match msg {
                    Some(russh::ChannelMsg::Data { data }) => {
                        let text = String::from_utf8_lossy(&data);
                        print!("{}", text);
                        output.push_str(&text);
                    }
                    Some(russh::ChannelMsg::Eof) => {
                        println!("\n[EOF received]");
                        break;
                    }
                    Some(russh::ChannelMsg::ExitStatus { exit_status }) => {
                        println!("[Exit status: {}]", exit_status);
                    }
                    Some(russh::ChannelMsg::Close) => {
                        println!("[Channel closed]");
                        break;
                    }
                    Some(other) => {
                        println!("[Other message: {:?}]", other);
                    }
                    None => {
                        println!("[Channel ended]");
                        break;
                    }
                }
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(5)) => {
                println!("[Timeout waiting for output]");
                break;
            }
        }
    }

    // Verify the output
    let trimmed = output.trim();
    println!("Command output: '{}'", trimmed);
    assert_eq!(trimmed, "root", "whoami should return 'root'");

    // Disconnect
    println!("Disconnecting...");
    session.disconnect(Disconnect::ByApplication, "", "en").await?;
    println!("Test passed!");

    Ok(())
}

#[tokio::test]
#[ignore = "requires a real SSH key file — run with `cargo test -- --ignored`"]
async fn test_ssh_key_file_validation() -> Result<()> {
    let key_path = Path::new(TEST_KEY_PATH);

    // Verify key file exists
    assert!(key_path.exists(), "SSH key file should exist");

    // Read and verify it's a valid OpenSSH key
    let content = std::fs::read_to_string(key_path)?;
    assert!(
        content.contains("-----BEGIN OPENSSH PRIVATE KEY-----"),
        "Should be an OpenSSH private key"
    );
    assert!(
        content.contains("-----END OPENSSH PRIVATE KEY-----"),
        "Should have proper key footer"
    );

    println!("SSH key file validation passed");
    Ok(())
}

#[tokio::test]
#[ignore = "requires a real SSH key file — run with `cargo test -- --ignored`"]
async fn test_key_decoding() -> Result<()> {
    use russh_keys::*;

    let key_path = Path::new(TEST_KEY_PATH);
    let private_key = std::fs::read_to_string(key_path)?;

    println!("Testing key decoding with passphrase...");
    let key_pair = decode_secret_key(&private_key, Some(TEST_KEY_PASSPHRASE))?;
    println!("Key type: {}", key_pair.name());
    println!("Key decoding test passed!");

    Ok(())
}
