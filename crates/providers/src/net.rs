//! TCP + TLS streams shared by IMAP and the HTTP-free protocols.

use std::pin::Pin;
use std::sync::{Arc, OnceLock};
use std::task::{Context, Poll};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_rustls::rustls::{self, pki_types::ServerName, ClientConfig};
use tokio_rustls::TlsConnector;

use crate::{Error, Result};

pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

pub enum MailStream {
    Plain(TcpStream),
    Tls(Box<TlsStream<TcpStream>>),
}

impl std::fmt::Debug for MailStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            MailStream::Plain(_) => "MailStream::Plain",
            MailStream::Tls(_) => "MailStream::Tls",
        })
    }
}

impl AsyncRead for MailStream {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            MailStream::Plain(s) => Pin::new(s).poll_read(cx, buf),
            MailStream::Tls(s) => Pin::new(s.as_mut()).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for MailStream {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<std::io::Result<usize>> {
        match self.get_mut() {
            MailStream::Plain(s) => Pin::new(s).poll_write(cx, buf),
            MailStream::Tls(s) => Pin::new(s.as_mut()).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            MailStream::Plain(s) => Pin::new(s).poll_flush(cx),
            MailStream::Tls(s) => Pin::new(s.as_mut()).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            MailStream::Plain(s) => Pin::new(s).poll_shutdown(cx),
            MailStream::Tls(s) => Pin::new(s.as_mut()).poll_shutdown(cx),
        }
    }
}

fn connector() -> TlsConnector {
    static CONFIG: OnceLock<Arc<ClientConfig>> = OnceLock::new();
    let config = CONFIG.get_or_init(|| {
        let roots = rustls::RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
        let config = ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .expect("ring supports the default TLS versions")
            .with_root_certificates(roots)
            .with_no_client_auth();
        Arc::new(config)
    });
    TlsConnector::from(Arc::clone(config))
}

pub async fn tcp(host: &str, port: u16) -> Result<TcpStream> {
    let stream = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect((host, port)))
        .await
        .map_err(|_| Error::Network(format!("timed out connecting to {host}:{port}")))??;
    stream.set_nodelay(true).ok();
    Ok(stream)
}

pub async fn upgrade(stream: TcpStream, host: &str) -> Result<MailStream> {
    let name = ServerName::try_from(host.to_string()).map_err(|e| Error::Network(e.to_string()))?;
    let tls = tokio::time::timeout(CONNECT_TIMEOUT, connector().connect(name, stream))
        .await
        .map_err(|_| Error::Network(format!("TLS handshake with {host} timed out")))?
        .map_err(|e| Error::Network(format!("TLS with {host}: {e}")))?;
    Ok(MailStream::Tls(Box::new(tls)))
}

pub async fn connect_tls(host: &str, port: u16) -> Result<MailStream> {
    upgrade(tcp(host, port).await?, host).await
}
