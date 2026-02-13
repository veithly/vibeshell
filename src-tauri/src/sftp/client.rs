//! SFTP Client implementation using russh-sftp.
//!
//! Provides the core SFTP session management including connection lifecycle
//! and subsystem initialization over an established SSH channel.

use anyhow::{Result, anyhow};
use russh_sftp::client::SftpSession;
use tokio::io::{AsyncRead, AsyncWrite};

/// SFTP client that wraps a russh-sftp session.
///
/// This client manages the SFTP subsystem connection lifecycle and provides
/// access to file transfer operations.
pub struct SftpClient {
    /// The underlying SFTP session, None if not connected.
    session: Option<SftpSession>,
}

impl SftpClient {
    /// Creates a new disconnected SFTP client.
    pub fn new() -> Self {
        Self {
            session: None,
        }
    }

    /// Connects to the SFTP subsystem using the provided stream.
    ///
    /// This method initializes the SFTP protocol on the given stream,
    /// which must be an already-established SSH channel stream with
    /// the SFTP subsystem requested.
    ///
    /// # Arguments
    ///
    /// * `stream` - A bidirectional stream (typically from `channel.into_stream()`)
    ///   that implements `AsyncRead + AsyncWrite + Unpin + Send`
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on successful connection, or an error if the
    /// SFTP subsystem cannot be initialized.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // After establishing SSH connection and requesting SFTP subsystem:
    /// let channel = session.channel_open_session().await?;
    /// channel.request_subsystem(true, "sftp").await?;
    /// let stream = channel.into_stream();
    /// sftp_client.connect(stream).await?;
    /// ```
    pub async fn connect<S>(&mut self, stream: S) -> Result<()>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        if self.session.is_some() {
            return Err(anyhow!("SFTP session already connected"));
        }

        let sftp_session = SftpSession::new(stream).await?;
        self.session = Some(sftp_session);

        Ok(())
    }

    /// Disconnects from the SFTP subsystem.
    ///
    /// This method closes the SFTP session and releases associated resources.
    /// It is safe to call on an already disconnected client.
    pub async fn disconnect(&mut self) -> Result<()> {
        if let Some(session) = self.session.take() {
            session.close().await?;
        }
        Ok(())
    }

    /// Checks if the SFTP client is currently connected.
    ///
    /// # Returns
    ///
    /// Returns `true` if there is an active SFTP session, `false` otherwise.
    pub fn is_connected(&self) -> bool {
        self.session.is_some()
    }

    /// Returns a reference to the underlying SFTP session if connected.
    ///
    /// This method provides access to the raw SFTP session for performing
    /// file operations.
    ///
    /// # Returns
    ///
    /// Returns `Some(&SftpSession)` if connected, `None` otherwise.
    pub fn session(&self) -> Option<&SftpSession> {
        self.session.as_ref()
    }

    /// Returns a mutable reference to the underlying SFTP session if connected.
    ///
    /// This method provides mutable access to the raw SFTP session for
    /// performing file operations that require mutable access.
    ///
    /// # Returns
    ///
    /// Returns `Some(&mut SftpSession)` if connected, `None` otherwise.
    pub fn session_mut(&mut self) -> Option<&mut SftpSession> {
        self.session.as_mut()
    }
}

impl Default for SftpClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_client_is_disconnected() {
        let client = SftpClient::new();
        assert!(!client.is_connected());
        assert!(client.session().is_none());
    }

    #[test]
    fn test_default_impl() {
        let client = SftpClient::default();
        assert!(!client.is_connected());
    }
}
