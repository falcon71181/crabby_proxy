use anyhow::anyhow;
use byte_pool::BytePool;
use lazy_static::lazy_static;
use tokio::net::TcpStream;

use crate::{error::ProxyError, utils};

use super::protocol::ProxyTarget;

const INITIAL_HTTP_HEADER_SIZE: usize = 1024;

lazy_static! {
    static ref BUFFER_POOL: BytePool::<Vec<u8>> = BytePool::<Vec<u8>>::new();
}

pub async fn handShake(
    mut client_stream: TcpStream,
    target: ProxyTarget,
) -> Result<Vec<u8>, ProxyError> {
    let str_addr = target.host;
    let mut buffer = BUFFER_POOL.alloc(INITIAL_HTTP_HEADER_SIZE);
    buffer.extend_from_slice("CONNECT ".as_bytes());
    buffer.extend_from_slice(str_addr.as_bytes());
    buffer.push(b':');
    buffer.extend_from_slice(target.port.to_string().as_bytes());
    buffer.extend_from_slice(" HTTP/1.1\r\n\r\n".as_bytes());

    utils::write_to_stream(&mut client_stream, buffer.as_ref()).await?;

    buffer.clear();

    let partially_read_body_start_index;
    loop {
        let mut tmp_buffer = [0u8; 256];
        let len = utils::read_from_stream(&mut client_stream, &mut tmp_buffer).await?;
        buffer.extend_from_slice(&tmp_buffer[..len]);
        let len = buffer.len();
        eprintln!("DEBUGPRINT[320]: relay.rs:37: len={:#?}", len);
        if len > 4 {
            let start_index = len - 4;
            if &buffer[start_index..len] == b"\r\n\r\n" {
                partially_read_body_start_index = Some(len);
                break;
            }

            if let Some(index) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
                partially_read_body_start_index = Some(index + 4);
                break;
            }
        }
    }

    if let Some(index) = partially_read_body_start_index {
        if buffer[..].starts_with("HTTP/1.1 200 OK".as_bytes()) {
            if index <= buffer.len() {
                return Ok(Vec::with_capacity(0));
            } else {
                return Ok(buffer[index..].into());
            }
        }
    }

    Err(ProxyError::BadGateway(anyhow!(
        "invalid response from proxy server"
    )))
}
