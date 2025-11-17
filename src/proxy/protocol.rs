use tokio::{io, net::TcpListener};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProxyProtocol {
    TCP,
    HTTP,
    HTTPS,
    SOCKS4,
    SOCKS5,
}

impl std::fmt::Display for ProxyProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            ProxyProtocol::TCP => "TCP",
            ProxyProtocol::HTTP => "HTTP",
            ProxyProtocol::HTTPS => "HTTPS",
            ProxyProtocol::SOCKS4 => "SOCKS4",
            ProxyProtocol::SOCKS5 => "SOCKS5",
        };
        write!(f, "{}", s)
    }
}

#[derive(Debug, Clone)]
pub struct ProxyTarget {
    pub host: String,
    pub port: u16,
}

pub struct MultiProtocolProxy {
    listener: TcpListener,
}

impl MultiProtocolProxy {
    pub async fn new(bind_addr: &str) -> io::Result<Self> {
        let listener = TcpListener::bind(bind_addr).await?;
        println!("Multi-protocol proxy listening on {}", bind_addr);
        Ok(Self { listener })
    }

    // pub async fn run(&self) -> io::Result<()> {
    //     loop {
    //         let (stream, addr) = self.listener.accept().await?;
    //         tokio::spawn(async move {
    //             if let Err(e) = handle_connection(stream, addr).await {
    //                 eprintln!("Error handling connection from {}: {}", addr, e);
    //             }
    //         });
    //     }
    // }
}
