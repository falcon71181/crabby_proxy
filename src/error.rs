#[derive(Debug)]
pub enum ProxyError {
    ConnectionRefused,
    IPv6NotSupported, // not supported by Socks4
    InternalError,
    BadRequest,
    Timeout,
    PayloadTooLarge,
    BadGateway(anyhow::Error),
    Disconnected(anyhow::Error),
}
