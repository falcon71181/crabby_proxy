use chrono::Utc;
use std::{collections::HashMap, net::SocketAddr};
use tokio::io::AsyncWriteExt;
use tokio::task::JoinHandle;
use tokio::{io, net::TcpStream};
use uuid::Uuid;

use crate::connection::ServiceType;

use super::{error::TunnelError, port_allocator::PortAllocator};

pub struct TunnelManager {
    active_tunnels: HashMap<u16, ActiveTunnel>,
    port_allocator: PortAllocator,
    tasks: HashMap<u16, Vec<JoinHandle<()>>>,
}

struct ActiveTunnel {
    tunnel_id: Uuid,
    client_id: Uuid,
    listen_port: u16,
    target_addr: SocketAddr,
    service_type: ServiceType,
    created_at: chrono::DateTime<Utc>,
}

impl TunnelManager {
    pub fn new(start_port: u16, end_port: u16) -> Self {
        Self {
            active_tunnels: HashMap::new(),
            port_allocator: PortAllocator::new(start_port, end_port),
            tasks: HashMap::new(),
        }
    }

    pub async fn create_reverse_tunnel(
        &mut self,
        service_type: ServiceType,
        preferred_port: Option<u16>,
        client_addr: SocketAddr,
    ) -> Result<u16, TunnelError> {
        // Allocate port
        let port = self.port_allocator.allocate_port(preferred_port)?;

        // Create tunnel
        let tunnel = ActiveTunnel {
            tunnel_id: Uuid::new_v4(),
            client_id: Uuid::new_v4(),
            listen_port: port,
            target_addr: client_addr,
            service_type: service_type.clone(),
            created_at: Utc::now(),
        };

        let handle = tokio::spawn(tunnel_listener_task(port, client_addr));
        self.tasks.entry(port).or_default().push(handle);

        // Store tunnel
        self.active_tunnels.insert(port, tunnel);
        Ok(port)
    }

    pub async fn close_tunnel(&mut self, port: u16) -> Result<(), TunnelError> {
        // Remove tunnel metadata first
        self.active_tunnels
            .remove(&port)
            .ok_or(TunnelError::TunnelNotFound(port))?;

        // Release port
        self.port_allocator
            .release_port(port)
            .map_err(|e| TunnelError::PortReleaseError(port, e.to_string()))?;

        // Abort tasks associated with this port only
        if let Some(handles) = self.tasks.remove(&port) {
            for handle in handles {
                handle.abort();
                // optionally: let _ = handle.await; // would yield JoinError::is_cancelled()
            }
        }

        Ok(())
    }

    pub async fn shutdown(&mut self) {
        for (_, handles) in self.tasks.drain() {
            for h in handles {
                h.abort();
            }
        }
        self.active_tunnels.clear();
    }
}

async fn tunnel_listener_task(port: u16, target_addr: SocketAddr) {
    let listener = match tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Failed to bind tunnel port {port}: {e}");
            return;
        }
    };

    loop {
        match listener.accept().await {
            Ok((inbound, _)) => {
                tokio::spawn(handle_tunnel_connection(inbound, target_addr));
            }
            Err(e) => {
                eprintln!("Accept error on port {port}: {e}");
            }
        }
    }
}

async fn handle_tunnel_connection(mut inbound: tokio::net::TcpStream, target_addr: SocketAddr) {
    let mut outbound = match TcpStream::connect(target_addr).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to connect to client {target_addr}: {e}");
            return;
        }
    };

    let (mut ri, mut wi) = inbound.split();
    let (mut ro, mut wo) = outbound.split();

    let client_to_server = async {
        io::copy(&mut ri, &mut wo).await?;
        wo.shutdown().await
    };

    let server_to_client = async {
        io::copy(&mut ro, &mut wi).await?;
        wi.shutdown().await
    };

    if let Err(e) = tokio::try_join!(client_to_server, server_to_client) {
        eprintln!("Tunnel error: {e}");
    }
}
