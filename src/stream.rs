use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpStream,
};

pub enum ClientStream {
    Plain(TcpStream),
    Tls(tokio_rustls::server::TlsStream<tokio::net::TcpStream>),
}

impl AsyncRead for ClientStream {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match &mut *self {
            ClientStream::Plain(stream) => std::pin::Pin::new(stream).poll_read(cx, buf),
            ClientStream::Tls(stream) => std::pin::Pin::new(stream).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for ClientStream {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        match &mut *self {
            ClientStream::Plain(stream) => std::pin::Pin::new(stream).poll_write(cx, buf),
            ClientStream::Tls(stream) => std::pin::Pin::new(stream).poll_write(cx, buf),
        }
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        match &mut *self {
            ClientStream::Plain(stream) => std::pin::Pin::new(stream).poll_flush(cx),
            ClientStream::Tls(stream) => std::pin::Pin::new(stream).poll_flush(cx),
        }
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        match &mut *self {
            ClientStream::Plain(stream) => std::pin::Pin::new(stream).poll_shutdown(cx),
            ClientStream::Tls(stream) => std::pin::Pin::new(stream).poll_shutdown(cx),
        }
    }
}

pub struct TunnelStream<R: AsyncRead, W: AsyncWrite> {
    read: R,
    write: W,
}

impl<R: AsyncRead, W: AsyncWrite> TunnelStream<R, W> {
    pub fn new(read: R, write: W) -> Self {
        Self { read, write }
    }

    /// Relay data from the read side to the write side
    pub async fn relay_with_logging(&mut self, label: &str) -> tokio::io::Result<u64>
    where
        R: AsyncReadExt + Unpin,
        W: AsyncWriteExt + Unpin,
    {
        let mut buf = [0u8; 1024];
        let mut total = 0;

        loop {
            let n = self.read.read(&mut buf).await?;
            if n == 0 {
                break;
            }

            tracing::trace!("{} - {} bytes", label, n);

            self.write.write_all(&buf[..n]).await?;
            total += n as u64;
        }

        self.write.shutdown().await?;
        Ok(total)
    }
}

impl<R: AsyncRead + Unpin, W: AsyncWrite + Unpin> AsyncRead for TunnelStream<R, W> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().read).poll_read(cx, buf)
    }
}

impl<R: AsyncRead + Unpin, W: AsyncWrite + Unpin> AsyncWrite for TunnelStream<R, W> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        Pin::new(&mut self.get_mut().write).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Pin::new(&mut self.get_mut().write).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Pin::new(&mut self.get_mut().write).poll_shutdown(cx)
    }
}

pub async fn relay_with_tunnel_stream<R, W>(
    mut tunnel: TunnelStream<R, W>,
    label: &str,
) -> tokio::io::Result<u64>
where
    R: AsyncRead + AsyncReadExt + Unpin,
    W: AsyncWrite + AsyncWriteExt + Unpin,
{
    tunnel.relay_with_logging(label).await
}

/// Create a bidirectional tunnel b/w two stream
pub async fn create_bidirectional_tunnel<R1, W1, R2, W2>(
    stream1: (R1, W1),
    stream2: (R2, W2),
    label1: &str,
    label2: &str,
) -> tokio::io::Result<(u64, u64)>
where
    R1: AsyncRead + AsyncReadExt + Unpin,
    W1: AsyncWrite + AsyncWriteExt + Unpin,
    R2: AsyncRead + AsyncReadExt + Unpin,
    W2: AsyncWrite + AsyncWriteExt + Unpin,
{
    let tunnel1 = TunnelStream::new(stream1.0, stream2.1);
    let tunnel2 = TunnelStream::new(stream2.0, stream1.1);

    let relay1 = relay_with_tunnel_stream(tunnel1, label1);
    let relay2 = relay_with_tunnel_stream(tunnel2, label2);

    tokio::try_join!(relay1, relay2)
}
