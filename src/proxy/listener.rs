use crate::app_state::AppState;
use crate::proxy::protocol::ProxyProtocol;
use crate::stream::{create_bidirectional_tunnel, ClientStream, TunnelStream};
use crate::utils;
use base64::Engine;
use std::net::SocketAddr;
use std::net::{Ipv4Addr, Ipv6Addr};
use tokio::io::{
    self, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader,
};
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::time::{timeout, Duration};

use super::protocol::ProxyTarget;

type AuthResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

// Error classification
#[derive(Debug, PartialEq)]
enum ErrorType {
    Handshake,
    Connection,
    Response,
    Timeout,
    Tunnel,
}

pub async fn run_proxy_server(state: AppState, addr: SocketAddr) {
    let listener = TcpListener::bind(addr).await.unwrap();
    while let Ok((client_stream, client_addr)) = listener.accept().await {
        let state = state.clone();
        tokio::spawn(async move {
            handle_client(client_stream, client_addr, state).await;
        });
    }
}

// Helper function to send error responses
async fn send_error_response(
    protocol: &ProxyProtocol,
    stream: &mut ClientStream,
    error_type: ErrorType,
) -> io::Result<()> {
    match (protocol, error_type) {
        (ProxyProtocol::HTTP, ErrorType::Handshake) => {
            stream.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n").await
        }
        (ProxyProtocol::HTTP, ErrorType::Connection) => {
            stream.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n").await
        }
        (ProxyProtocol::HTTP, ErrorType::Timeout) => {
            stream
                .write_all(b"HTTP/1.1 504 Gateway Timeout\r\n\r\n")
                .await
        }
        (ProxyProtocol::SOCKS4, _) => utils::send_socks4_response(stream, false).await,
        (ProxyProtocol::SOCKS5, _) => utils::send_socks5_response(stream, false).await,
        _ => Ok(()), // Unknown protocols don't send responses
    }
}

fn validate_auth_header(
    auth_header: &str,
    state: &AppState,
) -> Result<(), Box<dyn std::error::Error>> {
    if auth_header.starts_with("Basic ") {
        let encoded = &auth_header[6..]; // Skip "Basic " bytes
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|_| "Invalid base64 encoding")?;

        let credentials = String::from_utf8(decoded).map_err(|_| "Invalid UTF-8 in credentials")?;

        let mut parts = credentials.splitn(2, ':');
        let username = parts.next().ok_or("Missing username")?;
        let password = parts.next().ok_or("Missing password")?;

        if validate_credentials(username, password, state) {
            Ok(())
        } else {
            Err("Invalid credentials".into())
        }
    } else {
        Err("Unsupported authentication method".into())
    }
}

fn validate_credentials(username: &str, password: &str, state: &AppState) -> bool {
    let (expected_username, expected_password) = match (&state.username, &state.password) {
        (Some(user), Some(pass)) => (user, pass),
        _ => return false,
    };

    let username_ok = bool::from(expected_username.as_bytes().eq(username.as_bytes()));
    let password_ok = bool::from(expected_password.as_bytes().eq(password.as_bytes()));

    username_ok && password_ok
}

async fn handle_client(mut client_stream: TcpStream, client_addr: SocketAddr, state: AppState) {
    let mut protocol = ProxyProtocol::TCP;

    // First detect the protocol
    let protocol_detection_result = detect_client_protocol(&mut client_stream, &mut protocol).await;

    if let Err(e) = protocol_detection_result {
        tracing::error!("Protocol detection failed for {}: {}", client_addr, e);
        return;
    }

    // If protocol is HTTPS and we have TLS support, upgrade the connection
    let mut stream: ClientStream = if protocol == ProxyProtocol::HTTPS {
        match &state.tls_acceptor {
            Some(tls_acceptor) => match tls_acceptor.accept(client_stream).await {
                Ok(tls_stream) => {
                    tracing::debug!("TLS handshake successful for {}", client_addr);
                    ClientStream::Tls(tls_stream)
                }
                Err(e) => {
                    tracing::error!("TLS handshake failed for {}: {}", client_addr, e);
                    return;
                }
            },
            None => {
                tracing::error!(
                    "HTTPS protocol detected but no TLS configuration available for {}",
                    client_addr
                );
                return;
            }
        }
    } else {
        ClientStream::Plain(client_stream)
    };

    // If credentials are required, perform protocol-specific authentication
    if state.require_creds {
        match authenticate_by_protocol(&mut stream, &protocol, &state).await {
            Ok(true) => {
                tracing::debug!(
                    "{} authenticated successfully via {}",
                    &client_addr,
                    protocol
                );
            }
            Ok(false) => {
                // Authentication required for this protocol
                tracing::error!(
                    "Authentication required for {} via {}",
                    &client_addr,
                    protocol
                );
                return;
            }
            Err(e) => {
                tracing::error!("Auth failed for {} via {}: {}", client_addr, protocol, e);
                return;
            }
        }
    } else {
        tracing::debug!("Skipping authentication (--no-creds)");
    }

    let result = async_handle_client(&mut stream, client_addr, &mut protocol).await;

    if let Err((e, error_type)) = result {
        tracing::error!(
            "Error [{}] for {}: {}",
            match error_type {
                ErrorType::Handshake => "handshake",
                ErrorType::Connection => "connection",
                ErrorType::Response => "response",
                ErrorType::Timeout => "timeout",
                ErrorType::Tunnel => "tunnel",
            },
            client_addr,
            e
        );

        if error_type != ErrorType::Tunnel {
            let _ = send_error_response(&protocol, &mut stream, error_type).await;
        }
    }
}

async fn detect_client_protocol(
    client_stream: &mut TcpStream,
    protocol: &mut ProxyProtocol,
) -> Result<(), io::Error> {
    let mut peek_buf = [0u8; 4];

    match timeout(Duration::from_secs(5), client_stream.peek(&mut peek_buf)).await {
        Ok(Ok(_)) => {
            *protocol = detect_protocol(&peek_buf)
                .await
                .unwrap_or(ProxyProtocol::TCP);
            Ok(())
        }
        Ok(Err(e)) => Err(e),
        Err(_) => {
            *protocol = ProxyProtocol::TCP;
            Ok(())
        }
    }
}

async fn authenticate_by_protocol(
    client_stream: &mut ClientStream,
    protocol: &ProxyProtocol,
    state: &AppState,
) -> AuthResult<bool> {
    match protocol {
        ProxyProtocol::HTTP | ProxyProtocol::HTTPS => {
            authenticate_http_or_https(client_stream, state).await
        }
        ProxyProtocol::SOCKS5 => authenticate_socks5(client_stream, state).await,
        ProxyProtocol::SOCKS4 => authenticate_socks4(client_stream, state).await,
        _ => {
            // For TCP and other protocols that don't support authentication
            // We can reject them without auth
            tracing::warn!(
                "Protocol {} doesn't support authentication, rejecting connection",
                protocol
            );
            Ok(false)
        }
    }
}

async fn authenticate_http_or_https(
    client_stream: &mut ClientStream,
    state: &AppState,
) -> AuthResult<bool> {
    let mut buffer = [0u8; 4096];

    // Use peek equivalent for our stream type
    let n = match client_stream {
        ClientStream::Plain(stream) => stream.peek(&mut buffer).await?,
        ClientStream::Tls(stream) => {
            // For TLS streams, we need to read instead of peek
            // Save the position and data for later re-use
            let _pos = stream.get_ref().0.peer_addr()?; // TODO: This is a hack, need a better way
            stream.read(&mut buffer).await?
        }
    };

    if n == 0 {
        return Err("Connection closed by client".into());
    }

    let request_data = String::from_utf8_lossy(&buffer[..n]);

    // Extract Proxy-Authorization header from HTTP request
    if let Some(auth_header) = extract_proxy_auth_header(&request_data) {
        if validate_auth_header(auth_header, state).is_ok() {
            return Ok(true);
        }
    }

    // Authentication failed - send 407 response
    let _ = send_http_auth_required_response(client_stream).await;
    Err("HTTP authentication required".into())
}

fn extract_proxy_auth_header(request_data: &str) -> Option<&str> {
    for line in request_data.lines() {
        if line.to_lowercase().starts_with("proxy-authorization:") {
            return Some(line.trim_start_matches("Proxy-Authorization:").trim());
        }
    }
    None
}

async fn send_http_auth_required_response(
    client_stream: &mut ClientStream,
) -> Result<(), Box<dyn std::error::Error>> {
    let response = "HTTP/1.1 407 Proxy Authentication Required\r\n\
                   Proxy-Authenticate: Basic realm=\"Proxy\"\r\n\
                   Content-Length: 0\r\n\
                   Connection: close\r\n\
                   \r\n";

    client_stream.write_all(response.as_bytes()).await?;
    client_stream.flush().await?;
    Ok(())
}

async fn authenticate_socks4(
    client_stream: &mut ClientStream,
    state: &AppState,
) -> AuthResult<bool> {
    // Read SOCKS4 request
    let mut request_buf = [0u8; 8];
    client_stream.read_exact(&mut request_buf).await?;

    let version = request_buf[0];
    let _command = request_buf[1];

    if version != 0x04 {
        return Err("Invalid SOCKS4 version".into());
    }

    // SOCKS4 doesn't have proper authentication, but we can check the user ID field
    if state.require_creds {
        // Read user ID (null-terminated string)
        let mut user_id = Vec::new();
        let mut byte_buf = [0u8; 1];

        loop {
            client_stream.read_exact(&mut byte_buf).await?;
            if byte_buf[0] == 0 {
                break;
            }
            user_id.push(byte_buf[0]);
        }

        let user_id_str = String::from_utf8(user_id).unwrap_or_default();

        // For SOCKS4, we might implement a simple token-based auth using the user ID field
        // This is non-standard but commonly used as a workaround
        if validate_socks4_user_id(&user_id_str, state) {
            Ok(true)
        } else {
            // Send SOCKS4 rejection
            let mut response = vec![0x00, 0x5B]; // 0x00 (version), 0x5B (rejected)
            response.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // NULL address
            response.extend_from_slice(&[0x00, 0x00]); // NULL port
            client_stream.write_all(&response).await?;
            Err("SOCKS4 authentication failed".into())
        }
    } else {
        Ok(false)
    }
}

fn validate_socks4_user_id(user_id: &str, state: &AppState) -> bool {
    // Simple implementation: check if user_id matches expected username
    // You might want to implement a more sophisticated scheme
    if let Some(expected_username) = &state.username {
        bool::from(expected_username.as_bytes().eq(user_id.as_bytes()))
    } else {
        false
    }
}

async fn authenticate_socks5(
    client_stream: &mut ClientStream,
    state: &AppState,
) -> AuthResult<bool> {
    // Read SOCKS5 handshake
    let mut handshake_buf = [0u8; 2];
    client_stream.read_exact(&mut handshake_buf).await?;

    let version = handshake_buf[0];
    let num_methods = handshake_buf[1];

    if version != 0x05 {
        return Err("Invalid SOCKS version".into());
    }

    let mut methods_buf = vec![0u8; num_methods as usize];
    client_stream.read_exact(&mut methods_buf).await?;

    // Check if username/password auth is supported by client
    let supports_auth = methods_buf.contains(&0x02);

    if state.require_creds && supports_auth {
        // Tell client to use username/password auth
        client_stream.write_all(&[0x05, 0x02]).await?;

        // Read auth request
        let mut auth_buf = [0u8; 2];
        client_stream.read_exact(&mut auth_buf).await?;

        let auth_version = auth_buf[0];
        if auth_version != 0x01 {
            return Err("Unsupported SOCKS5 auth version".into());
        }

        let username_len = auth_buf[1] as usize;
        let mut username_buf = vec![0u8; username_len];
        client_stream.read_exact(&mut username_buf).await?;

        let mut pass_len_buf = [0u8; 1];
        client_stream.read_exact(&mut pass_len_buf).await?;
        let password_len = pass_len_buf[0] as usize;

        let mut password_buf = vec![0u8; password_len];
        client_stream.read_exact(&mut password_buf).await?;

        let username = String::from_utf8(username_buf)?;
        let password = String::from_utf8(password_buf)?;

        // Validate credentials
        if validate_credentials(&username, &password, state) {
            client_stream.write_all(&[0x01, 0x00]).await?; // Success
            Ok(true)
        } else {
            client_stream.write_all(&[0x01, 0x01]).await?; // Failure
            Err("SOCKS5 authentication failed".into())
        }
    } else if state.require_creds {
        // Client doesn't support auth but we require it
        client_stream.write_all(&[0x05, 0xFF]).await?; // No acceptable methods
        Err("SOCKS5 authentication required but not supported by client".into())
    } else {
        // No auth required
        client_stream.write_all(&[0x05, 0x00]).await?; // No authentication
        Ok(false)
    }
}

async fn async_handle_client(
    client_stream: &mut ClientStream,
    client_addr: SocketAddr,
    protocol: &mut ProxyProtocol,
) -> Result<(), (io::Error, ErrorType)> {
    let mut peek_buf = [0u8; 4];
    let peek_result = timeout(Duration::from_secs(5), async {
        match client_stream {
            ClientStream::Plain(stream) => stream.peek(&mut peek_buf).await,
            ClientStream::Tls(stream) => {
                // For TLS streams, we need to read instead of peek
                // Save the position and data for later re-use
                let _pos = stream.get_ref().0.peer_addr()?; // TODO: This is a hack, need a better way
                let n = stream.read(&mut peek_buf).await?;
                Ok(n)
            }
        }
    })
    .await;

    *protocol = match peek_result {
        Ok(Ok(_n)) => detect_protocol(&peek_buf)
            .await
            .unwrap_or(ProxyProtocol::TCP),
        Ok(Err(_)) | Err(_) => ProxyProtocol::TCP,
    };

    let target = parse_target_by_protocol(client_stream, protocol)
        .await
        .map_err(|e| (e, ErrorType::Handshake))?;

    let target_addr = format!("{}:{}", target.host, target.port);
    let target_stream = timeout(Duration::from_secs(10), TcpStream::connect(&target_addr))
        .await
        .map_err(|_| {
            (
                io::Error::new(io::ErrorKind::TimedOut, "Connection timeout"),
                ErrorType::Timeout,
            )
        })?
        .map_err(|e| (e, ErrorType::Connection))?;

    tracing::info!(
        "Connection established to {} by {}",
        target_addr,
        client_addr
    );

    // Send success response
    match *protocol {
        ProxyProtocol::HTTP => utils::send_http_connect_response(client_stream).await,
        ProxyProtocol::SOCKS4 => utils::send_socks4_response(client_stream, true).await,
        ProxyProtocol::SOCKS5 => utils::send_socks5_response(client_stream, true).await,
        _ => Ok(()),
    }
    .map_err(|e| (e, ErrorType::Response))?;

    let client_halves = io::split(client_stream);
    let target_halves = io::split(target_stream);

    let label_c2t = format!("[{}]: C[{}]->T[{}]", protocol, client_addr, target_addr);
    let label_t2c = format!("[{}]: T[{}]->C[{}]", protocol, target_addr, client_addr);

    match create_bidirectional_tunnel(client_halves, target_halves, &label_c2t, &label_t2c).await {
        Ok((c2t, t2c)) => {
            tracing::info!(
                "Closed tunnel {} <-> {} (sent: {}, received: {})",
                client_addr,
                target_addr,
                c2t,
                t2c
            );
            Ok(())
        }
        Err(e) => {
            tracing::warn!("Tunnel error: {}", e);
            Err((e, ErrorType::Tunnel))
        }
    }
}

/// Relay data using TunnelStream
///
/// label is a tag for the direction, e.g., "C->T" (client to target).
async fn relay_with_tunnel_stream<R, W>(
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

async fn detect_protocol(peek_buf: &[u8; 4]) -> io::Result<ProxyProtocol> {
    // HTTP methods start with ASCII letters
    if peek_buf.starts_with(b"GET ")
        || peek_buf.starts_with(b"POST")
        || peek_buf.starts_with(b"PUT ")
        || peek_buf.starts_with(b"HEAD")
        || peek_buf.starts_with(b"DELE")
        || peek_buf.starts_with(b"CONN")
    {
        return Ok(ProxyProtocol::HTTP);
    }

    // HTTPS/TLS starts with 0x16 (handshake)
    if peek_buf[0] == 0x16 {
        return Ok(ProxyProtocol::HTTPS);
    }

    // SOCKS5 starts with version 0x05
    if peek_buf[0] == 0x05 {
        return Ok(ProxyProtocol::SOCKS5);
    }

    // SOCKS4 starts with version 0x04
    if peek_buf[0] == 0x04 {
        return Ok(ProxyProtocol::SOCKS4);
    }

    // Default to TCP for unknown protocols
    Ok(ProxyProtocol::TCP)
}

async fn parse_target_by_protocol(
    stream: &mut ClientStream,
    protocol: &ProxyProtocol,
) -> io::Result<ProxyTarget> {
    match protocol {
        ProxyProtocol::HTTP => parse_http_target_from_stream(stream).await,
        ProxyProtocol::HTTPS => parse_https_target_from_stream(stream).await,
        ProxyProtocol::SOCKS4 => parse_socks4_target(stream).await,
        ProxyProtocol::SOCKS5 => parse_socks5_target(stream).await,
        ProxyProtocol::TCP => parse_target(stream).await, // Your original function
    }
}

async fn parse_http_target_from_stream(stream: &mut ClientStream) -> io::Result<ProxyTarget> {
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    reader.read_line(&mut request_line).await?;

    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 2 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid HTTP request",
        ));
    }

    let method = parts[0];
    let url = parts[1];

    if method == "CONNECT" {
        parse_connect_target(url)
    } else {
        parse_http_target(url)
    }
}

async fn parse_https_target_from_stream(stream: &mut ClientStream) -> io::Result<ProxyTarget> {
    // For HTTPS, we need to parse SNI from TLS handshake
    // This is a simplified version - you'd need full TLS parsing for production
    let mut buf = vec![0u8; 512];
    // Use peek equivalent for our stream type
    let n = match stream {
        ClientStream::Plain(stream) => stream.peek(&mut buf).await?,
        ClientStream::Tls(stream) => {
            // For TLS streams, we need to read instead of peek
            // Save the position and data for later re-use
            let _pos = stream.get_ref().0.peer_addr()?; // TODO: This is a hack, need a better way
            stream.read(&mut buf).await?
        }
    };

    if let Some(sni) = extract_sni_from_tls(&buf[..n]) {
        Ok(ProxyTarget {
            host: sni,
            port: 443,
        })
    } else {
        // Fallback if SNI extraction fails
        Ok(ProxyTarget {
            host: "unknown.host".to_string(),
            port: 443,
        })
    }
}

async fn parse_socks4_target(stream: &mut ClientStream) -> io::Result<ProxyTarget> {
    let mut buf = [0u8; 8];
    stream.read_exact(&mut buf).await?;

    let version = buf[0];
    let command = buf[1];
    let port = u16::from_be_bytes([buf[2], buf[3]]);
    let ip = Ipv4Addr::from([buf[4], buf[5], buf[6], buf[7]]);

    if version != 0x04 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid SOCKS4 version",
        ));
    }

    if command != 0x01 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Only CONNECT supported",
        ));
    }

    // Read user ID (null-terminated)
    let mut user_id = Vec::new();
    loop {
        let mut byte = [0u8; 1];
        stream.read_exact(&mut byte).await?;
        if byte[0] == 0 {
            break;
        }
        user_id.push(byte[0]);
    }

    Ok(ProxyTarget {
        host: ip.to_string(),
        port,
    })
}

async fn parse_socks5_target(stream: &mut ClientStream) -> io::Result<ProxyTarget> {
    // Handle authentication negotiation first
    let mut buf = [0u8; 2];
    stream.read_exact(&mut buf).await?;

    let version = buf[0];
    let n_methods = buf[1];

    if version != 0x05 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid SOCKS5 version",
        ));
    }

    // Read authentication methods
    let mut methods = vec![0u8; n_methods as usize];
    stream.read_exact(&mut methods).await?;

    // Send "no authentication required" response
    stream.write_all(&[0x05, 0x00]).await?;

    // Read connection request
    let mut req_buf = [0u8; 4];
    stream.read_exact(&mut req_buf).await?;

    let version = req_buf[0];
    let command = req_buf[1];
    let _reserved = req_buf[2];
    let addr_type = req_buf[3];

    if version != 0x05 || command != 0x01 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid SOCKS5 request",
        ));
    }

    let target = match addr_type {
        0x01 => {
            // IPv4
            let mut addr_buf = [0u8; 6];
            stream.read_exact(&mut addr_buf).await?;
            let ip = Ipv4Addr::from([addr_buf[0], addr_buf[1], addr_buf[2], addr_buf[3]]);
            let port = u16::from_be_bytes([addr_buf[4], addr_buf[5]]);
            ProxyTarget {
                host: ip.to_string(),
                port,
            }
        }
        0x03 => {
            // Domain name
            let mut len_buf = [0u8; 1];
            stream.read_exact(&mut len_buf).await?;
            let len = len_buf[0] as usize;

            let mut domain_buf = vec![0u8; len];
            stream.read_exact(&mut domain_buf).await?;
            let domain = String::from_utf8(domain_buf)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid domain"))?;

            let mut port_buf = [0u8; 2];
            stream.read_exact(&mut port_buf).await?;
            let port = u16::from_be_bytes(port_buf);

            ProxyTarget { host: domain, port }
        }
        0x04 => {
            // IPv6
            let mut addr_buf = [0u8; 18];
            stream.read_exact(&mut addr_buf).await?;
            let ip_bytes: [u8; 16] = addr_buf[0..16].try_into().unwrap();
            let ip = Ipv6Addr::from(ip_bytes);
            let port = u16::from_be_bytes([addr_buf[16], addr_buf[17]]);
            ProxyTarget {
                host: ip.to_string(),
                port,
            }
        }
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Unsupported address type",
            ));
        }
    };

    Ok(target)
}

// Simplified SNI extraction (you'd want a proper TLS parser for production)
fn extract_sni_from_tls(data: &[u8]) -> Option<String> {
    // This is a very basic SNI extraction - in production you'd use a proper TLS library
    if data.len() < 43 || data[0] != 0x16 {
        return None;
    }

    // Look for SNI extension in TLS handshake
    // This is simplified and may not work for all cases
    for i in 0..data.len().saturating_sub(10) {
        if data[i..i + 4] == [0x00, 0x00, 0x00, 0x00] {
            // Server name extension
            if let Some(len_pos) = i.checked_add(9) {
                if len_pos < data.len() {
                    let name_len = data[len_pos] as usize;
                    if let Some(name_start) = len_pos.checked_add(1) {
                        if name_start + name_len <= data.len() {
                            if let Ok(hostname) =
                                String::from_utf8(data[name_start..name_start + name_len].to_vec())
                            {
                                return Some(hostname);
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

async fn parse_target(stream: &mut ClientStream) -> io::Result<ProxyTarget> {
    // Robust parsing with timeout and error handling
    let timeout = tokio::time::Duration::from_secs(5);

    let target = tokio::time::timeout(timeout, async {
        let mut length_buf = [0u8; 2];
        match stream.read_exact(&mut length_buf).await {
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "Client disconnected before sending length",
                ));
            }
            Err(e) => return Err(e),
        }

        let length = u16::from_be_bytes(length_buf) as usize;
        if length == 0 || length > 1024 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid target length",
            ));
        }

        let mut target_buf = vec![0u8; length];
        match stream.read_exact(&mut target_buf).await {
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    format!(
                        "Client disconnected while sending target data (expected {} bytes)",
                        length
                    ),
                ));
            }
            Err(e) => return Err(e),
        }

        let target_str = String::from_utf8(target_buf)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid UTF-8 in target"))?;

        let parts: Vec<&str> = target_str.split(':').collect();
        if parts.len() != 2 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid target format (expected host:port)",
            ));
        }

        let host = parts[0].to_string();
        let port = parts[1]
            .parse::<u16>()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid port number"))?;

        Ok(ProxyTarget { host, port })
    })
    .await
    .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "Timeout reading target"))??;

    Ok(target)
}

fn parse_connect_target(url: &str) -> io::Result<ProxyTarget> {
    let parts: Vec<&str> = url.split(':').collect();
    if parts.len() != 2 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid CONNECT target",
        ));
    }

    let host = parts[0].to_string();
    let port = parts[1]
        .parse::<u16>()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid port"))?;

    Ok(ProxyTarget { host, port })
}

fn parse_http_target(url: &str) -> io::Result<ProxyTarget> {
    if url.starts_with("http://") {
        let url_without_scheme = &url[7..];
        let parts: Vec<&str> = url_without_scheme
            .split('/')
            .next()
            .unwrap()
            .split(':')
            .collect();
        let host = parts[0].to_string();
        let port = if parts.len() > 1 {
            parts[1].parse::<u16>().unwrap_or(80)
        } else {
            80
        };
        Ok(ProxyTarget { host, port })
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid HTTP URL",
        ))
    }
}
