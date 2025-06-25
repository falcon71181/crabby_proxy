use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub struct TunnelStream<R: AsyncRead, W: AsyncWrite> {
    read: R,
    write: W,
}

impl<R: AsyncRead, W: AsyncWrite> TunnelStream<R, W> {
    pub fn new(read: R, write: W) -> Self {
        Self { read, write }
    }
}

impl<R: AsyncReadExt, W: AsyncWriteExt> TunnelStream<R, W> {
    /// Relay data using TunnelStream
    ///
    /// label is a tag for the direction, e.g., "C->T" (client to target).
    pub async fn relay_with_tunnel_stream(
        mut tunnel: TunnelStream<R, W>,
        label: &str,
    ) -> tokio::io::Result<u64>
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        let mut buf = [0u8; 1024];
        let mut total = 0;

        loop {
            let n = tunnel.read(&mut buf).await?;
            if n == 0 {
                break;
            }

            tracing::debug!("{}", label);

            tunnel.write_all(&buf[..n]).await?;
            total += n as u64;
        }

        tunnel.shutdown().await?;
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
