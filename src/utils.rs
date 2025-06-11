use anyhow::{Context, Result};
use tokio::{io::AsyncReadExt, io::AsyncWriteExt, net::TcpStream};

use crate::error::ProxyError;

// write from tcp stream
pub async fn write_to_stream(stream: &mut TcpStream, buf: &[u8]) -> Result<(), ProxyError> {
    stream
        .write(buf)
        .await
        .context(format!(
            "failed to write to stream, addr: {:?}",
            stream.peer_addr()
        ))
        .map_err(ProxyError::Disconnected)?;
    Ok(())
}

// read from tcp stream
pub async fn read_from_stream(stream: &mut TcpStream, buf: &mut [u8]) -> Result<usize, ProxyError> {
    let size = stream
        .read(buf)
        .await
        .context(format!(
            "failed to read from stream, addr: {:?}",
            stream.peer_addr()
        ))
        .map_err(ProxyError::Disconnected)?;
    Ok(size)
}

/// Sends an HTTP 200 OK response for a successful `CONNECT` method request.
///
/// This function writes a minimal HTTP response used to acknowledge a successful
/// tunnel establishment in response to a `CONNECT` request (common in HTTP proxies).
///
/// The response sent is:
///
/// ```http
/// HTTP/1.1 200 OK\r\n\r\n
/// ```
///
/// This tells the client that the TCP tunnel has been successfully established
/// and it may now start sending arbitrary data through the connection.
///
/// This is especially relevant when implementing a **man-in-the-middle proxy** or
/// **forward proxy** for HTTPS traffic, where the client expects a valid `200 OK`
/// after issuing a `CONNECT` request.
pub async fn send_http_connect_response(stream: &mut TcpStream) -> tokio::io::Result<()> {
    stream.write_all(b"HTTP/1.1 200 OK\r\n\r\n").await
}

/// Sends a SOCKS4 response to the client over the given TCP stream.
///
/// The SOCKS4 reply format is exactly 8 bytes, structured as:
///
/// ```text
/// +----+----+----+----+----+----+----+----+
/// | VN | CD | DSTPORT           | DSTIP   |
/// +----+----+----+----+----+----+----+----+
///   1    1     2 bytes             4 bytes
/// ```
///
/// - `VN` is always `0x00` in the reply.
/// - `CD` is `0x5A` for success or `0x5B` for failure.
/// - `DSTPORT` and `DSTIP` are typically set to `0x0000` and `0.0.0.0` respectively.
/// -  SOCKS4 response format: [VN, CD, DSTPORT (2 bytes), DSTIP (4 bytes)]
/// -  VN is always 0x00, CD is 0x5a (success) or 0x5b (failure)
pub async fn send_socks4_response(stream: &mut TcpStream, success: bool) -> tokio::io::Result<()> {
    let response = if success {
        [0x00, 0x5a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00] // Success
                                                         // 0 90 0.0.0.0:00
    } else {
        [0x00, 0x5b, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00] // Failure
                                                         // 0 91 0.0.0.0:00
    };
    stream.write_all(&response).await
}

/// Sends a SOCKS5 response to the client over the given TCP stream.
///
/// The SOCKS5 reply format is 10+ bytes for IPv4, structured as:
///
/// ```text
/// +----+-----+-------+------+----------+----------+
/// |VER | REP |  RSV  | ATYP | BND.ADDR | BND.PORT |
/// +----+-----+-------+------+----------+----------+
///   1     1     1       1       4 bytes    2 bytes
/// ```
///
/// - `VER` is `0x05` for SOCKS5.
/// - `REP` is `0x00` for success, or other error codes (e.g., `0x01` for general failure).
/// - `RSV` is reserved and must be `0x00`.
/// - `ATYP` is `0x01` for IPv4.
/// - `BND.ADDR` and `BND.PORT` are typically `0.0.0.0:0` if unused.
pub async fn send_socks5_response(stream: &mut TcpStream, success: bool) -> tokio::io::Result<()> {
    let response = if success {
        [0x05, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00] // Success
                                                                     // 5 0 0 1 0.0.0.0:00
    } else {
        [0x05, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00] // General failure
                                                                     // 5 1 0 1 0.0.0.0:00
    };
    stream.write_all(&response).await
}
