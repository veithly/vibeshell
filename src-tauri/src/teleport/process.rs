//! Bounded, cancellation-safe subprocess I/O; no shell and no payload logging.
use anyhow::{anyhow, bail, Context, Result};
use std::{
    process::{ExitStatus, Stdio},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt, DuplexStream},
    process::Command,
    sync::watch,
};

#[derive(Debug)]
pub(super) struct Output {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

pub(super) async fn cancelled(mut signal: watch::Receiver<bool>) {
    loop {
        if *signal.borrow() {
            return;
        }
        if signal.changed().await.is_err() {
            return;
        }
    }
}

async fn read_bounded(reader: &mut (impl AsyncRead + Unpin), cap: usize) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut chunk = [0; 8192];
    loop {
        let n = reader.read(&mut chunk).await?;
        if n == 0 {
            return Ok(output);
        }
        if output.len().saturating_add(n) > cap {
            bail!("Teleport process output limit exceeded");
        }
        output.extend_from_slice(&chunk[..n]);
    }
}

pub(super) async fn run(
    binary: &str,
    args: &[String],
    input: Option<&[u8]>,
    deadline: Duration,
    cap: usize,
    signal: watch::Receiver<bool>,
) -> Result<Output> {
    if *signal.borrow() {
        bail!("Teleport operation cancelled");
    }
    if input.is_some_and(|v| v.len() > 8 * 1024 * 1024) {
        bail!("Teleport input limit exceeded");
    }
    let mut child = Command::new(binary)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context("Cannot start tsh; install a compatible Teleport client and log in explicitly")?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("tsh stdin unavailable"))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("tsh stdout unavailable"))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("tsh stderr unavailable"))?;
    let operation = async {
        let (_, stdout, stderr, status) = tokio::try_join!(
            async move {
                stdin.write_all(input.unwrap_or_default()).await?;
                stdin.shutdown().await?;
                // ChildStdin::shutdown does not necessarily close an OS pipe.
                // Drop the owned handle now so commands reading to EOF can
                // finish while stdout/stderr and exit status are being drained.
                drop(stdin);
                Ok::<_, anyhow::Error>(())
            },
            read_bounded(&mut stdout, cap),
            read_bounded(&mut stderr, cap),
            async { Ok::<_, anyhow::Error>(child.wait().await?) },
        )?;
        if stdout.len().saturating_add(stderr.len()) > cap {
            bail!("Teleport combined output limit exceeded");
        }
        Ok(Output {
            status,
            stdout,
            stderr,
        })
    };
    let result = tokio::select! {
        result = tokio::time::timeout(deadline, operation) => match result { Ok(result) => result, Err(_) => Err(anyhow!("Teleport operation timed out")) },
        _ = cancelled(signal) => Err(anyhow!("Teleport operation cancelled")),
    };
    if result.is_err() {
        let _ = child.start_kill();
        let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
    }
    result
}

/// Binary protocol transport. Memory is bounded by duplex capacity; stderr is
/// concurrently discarded with a cap, never mixed into the SFTP byte stream.
/// Dropping the protocol client, closing the session or a pipe error kills and
/// reaps the child. SFTP request/handshake timeouts are handled by the caller.
pub(super) async fn stream(
    binary: &str,
    args: &[String],
    signal: watch::Receiver<bool>,
) -> Result<DuplexStream> {
    if *signal.borrow() {
        bail!("Teleport session is closed");
    }
    let mut child = Command::new(binary)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context("Cannot start tsh SFTP transport")?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("tsh stdin unavailable"))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("tsh stdout unavailable"))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("tsh stderr unavailable"))?;
    let (client, bridge) = tokio::io::duplex(64 * 1024);
    tokio::spawn(async move {
        let (mut reader, mut writer) = tokio::io::split(bridge);
        tokio::select! {
            _ = tokio::io::copy(&mut reader, &mut stdin) => {},
            _ = tokio::io::copy(&mut stdout, &mut writer) => {},
            _ = async {
                if read_bounded(&mut stderr, 1024 * 1024).await.is_ok() {
                    // Closing stderr is not EOF on the binary protocol stream.
                    std::future::pending::<()>().await;
                }
            } => {},
            _ = cancelled(signal) => {},
        }
        let _ = child.start_kill();
        let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
    });
    Ok(client)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn python() -> &'static str {
        if cfg!(windows) {
            "python"
        } else {
            "python3"
        }
    }
    fn args(code: &str) -> Vec<String> {
        vec!["-c".into(), code.into()]
    }
    #[tokio::test]
    async fn drains_output_while_writing_large_stdin_and_preserves_binary_bytes() {
        let signal = watch::channel(false).0;
        let input = vec![42; 512 * 1024];
        let output = run(python(), &args("import sys; sys.stdout.buffer.write(bytes(range(256))*1024); sys.stdout.flush(); sys.stderr.write('e'*131072); sys.stderr.flush(); d=sys.stdin.buffer.read(); sys.stdout.buffer.write(d)"), Some(&input), Duration::from_secs(10), 2 * 1024 * 1024, signal.subscribe()).await.unwrap();
        assert!(output.status.success());
        assert_eq!(&output.stdout[..256], &(0..=255u8).collect::<Vec<_>>());
        assert_eq!(&output.stdout[256 * 1024..], &input);
        assert_eq!(output.stderr.len(), 131072);
    }
    #[tokio::test]
    async fn failure_timeout_output_limit_and_cancellation_are_real_errors() {
        let signal = watch::channel(false).0;
        let result = run(
            python(),
            &args("import sys; sys.exit(7)"),
            None,
            Duration::from_secs(5),
            1024,
            signal.subscribe(),
        )
        .await
        .unwrap();
        assert_eq!(result.status.code(), Some(7));
        assert!(run(
            python(),
            &args("import time; time.sleep(30)"),
            None,
            Duration::from_millis(150),
            1024,
            signal.subscribe()
        )
        .await
        .is_err());
        assert!(run(
            python(),
            &args("print('x'*100000)"),
            None,
            Duration::from_secs(5),
            1024,
            signal.subscribe()
        )
        .await
        .is_err());
        let sleeping = args("import time; time.sleep(30)");
        let operation = run(
            python(),
            &sleeping,
            None,
            Duration::from_secs(30),
            1024,
            signal.subscribe(),
        );
        tokio::pin!(operation);
        tokio::select! { _ = &mut operation => panic!("finished before cancellation"), _ = tokio::time::sleep(Duration::from_millis(100)) => {} }
        signal.send_replace(true);
        assert!(tokio::time::timeout(Duration::from_secs(3), operation)
            .await
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("cancelled"));
    }
}
