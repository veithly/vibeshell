use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Drain both directions independently: a request-side FIN is not the end
/// of its response. Cancellation drops both streams without detached tasks.
pub(crate) async fn bridge<A, B>(
    a: A,
    b: B,
    bytes_in: &AtomicU64,
    bytes_out: &AtomicU64,
) -> std::io::Result<()>
where
    A: AsyncRead + AsyncWrite + Unpin,
    B: AsyncRead + AsyncWrite + Unpin,
{
    async fn copy<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
        mut reader: R,
        mut writer: W,
        counter: &AtomicU64,
    ) -> std::io::Result<()> {
        let mut buffer = [0u8; 32768];
        loop {
            let count = reader.read(&mut buffer).await?;
            if count == 0 {
                return writer.shutdown().await;
            }
            writer.write_all(&buffer[..count]).await?;
            counter.fetch_add(count as u64, Ordering::Relaxed);
        }
    }
    let (ar, aw) = tokio::io::split(a);
    let (br, bw) = tokio::io::split(b);
    tokio::try_join!(copy(ar, bw, bytes_in), copy(br, aw, bytes_out))?;
    Ok(())
}

pub(crate) struct ActiveConnection<'a>(&'a AtomicU32);
impl<'a> ActiveConnection<'a> {
    pub(crate) fn new(count: &'a AtomicU32) -> Self {
        count.fetch_add(1, Ordering::Relaxed);
        Self(count)
    }
}
impl Drop for ActiveConnection<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn half_close_preserves_the_complete_delayed_response() {
        let (mut client, left) = tokio::io::duplex(128);
        let (right, mut server) = tokio::io::duplex(128);
        let incoming = AtomicU64::new(0);
        let outgoing = AtomicU64::new(0);
        let forward = bridge(left, right, &incoming, &outgoing);
        let request = async {
            client.write_all(b"request").await.unwrap();
            client.shutdown().await.unwrap();
            let mut response = Vec::new();
            client.read_to_end(&mut response).await.unwrap();
            assert_eq!(response, vec![b'x'; 65536]);
        };
        let response = async {
            let mut request = Vec::new();
            server.read_to_end(&mut request).await.unwrap();
            assert_eq!(request, b"request");
            tokio::task::yield_now().await;
            server.write_all(&vec![b'x'; 65536]).await.unwrap();
            server.shutdown().await.unwrap();
        };
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let (result, _, _) = tokio::join!(forward, request, response);
            result.unwrap();
        })
        .await
        .unwrap();
        assert_eq!(incoming.load(Ordering::Relaxed), 7);
        assert_eq!(outgoing.load(Ordering::Relaxed), 65536);
    }

    #[test]
    fn active_count_is_balanced_on_scope_exit() {
        let count = AtomicU32::new(0);
        {
            let _connection = ActiveConnection::new(&count);
            assert_eq!(count.load(Ordering::Relaxed), 1);
        }
        assert_eq!(count.load(Ordering::Relaxed), 0);
    }
}
