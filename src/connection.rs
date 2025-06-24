use std::{collections::HashMap, net::SocketAddr, time::Instant};
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::proxy::protocol::ProxyTarget;

#[derive(Debug)]
pub enum ConnectionState {
    Pending,
    Approved,
    Rejected,
    Active,
    Closed,
}

// ConnectionType enum
#[derive(Debug)]
pub enum ConnectionType {
    /// Forward proxy (client -> proxy -> target server)
    Forward {
        target: ProxyTarget,
        protocol: ProxyProtocol,
    },
    /// Reverse tunnel (client requests proxy to expose a local service)
    ReverseTunnel {
        service_type: ServiceType,
        listen_port: Option<u16>, // None = auto-assign port
    },
}

// Supported protocols
#[derive(Debug)]
pub enum ProxyProtocol {
    Http,
    Https,
    Socks5,
    Tcp, // Raw TCP
    Ssh,
    Custom(String),
}

// Service types for reverse tunnels
#[derive(Debug)]
pub enum ServiceType {
    Database(DbType),
    WebService,
    SshService,
    Custom(String),
}

#[derive(Debug)]
pub enum DbType {
    Postgres,
    MySQL,
    Redis,
    MongoDB,
    Custom(String),
}

#[derive(Debug)]
pub struct ConnectionRequest {
    pub id: Uuid,
    pub client_addr: SocketAddr,
    pub connection_type: ConnectionType,
    pub state: ConnectionState,
    pub decision_tx: Option<oneshot::Sender<ConnectionApproval>>,
}

// Expanded approval response
#[derive(Debug)]
pub enum ConnectionApproval {
    Approved,
    ApprovedWithPort(u16), // For reverse tunnels
    Rejected(String),      // Rejection reason
}

#[derive(Debug)]
pub struct ConnectionManager {
    pending: HashMap<Uuid, ConnectionRequest>,
    active: HashMap<Uuid, Instant>,
}

#[derive(Debug)]
pub enum ConnectionError {
    NotFound,
    InvalidState,
}

impl ConnectionRequest {
    pub fn new(
        client_addr: SocketAddr,
        connection_type: ConnectionType,
        decision_tx: Option<oneshot::Sender<ConnectionApproval>>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            client_addr,
            connection_type,
            decision_tx,
            state: ConnectionState::Pending,
        }
    }
}

impl ConnectionManager {
    pub fn new() -> Self {
        Self {
            pending: HashMap::new(),
            active: HashMap::new(),
        }
    }

    pub fn add_pending(&mut self, request: ConnectionRequest) {
        self.pending.insert(request.id, request);
    }

    pub fn approve_connection(&mut self, id: Uuid) -> Result<(), ConnectionError> {
        let request = self.pending.get_mut(&id).ok_or(ConnectionError::NotFound)?;

        if !matches!(request.state, ConnectionState::Pending) {
            return Err(ConnectionError::InvalidState);
        }

        request.state = ConnectionState::Approved;

        if let Some(tx) = request.decision_tx.take() {
            let _ = tx.send(ConnectionApproval::Approved); // Ignore send errors
        }

        Ok(())
    }

    pub fn reject_connection(
        &mut self,
        id: Uuid,
        rejection_reason: String,
    ) -> Result<(), ConnectionError> {
        let mut request = self.pending.remove(&id).ok_or(ConnectionError::NotFound)?;

        if !matches!(request.state, ConnectionState::Pending) {
            return Err(ConnectionError::InvalidState);
        }

        request.state = ConnectionState::Rejected;

        if let Some(tx) = request.decision_tx.take() {
            let _ = tx.send(ConnectionApproval::Rejected(rejection_reason));
        }

        // Optionally drop or store rejected connections
        Ok(())
    }

    pub fn activate_connection(&mut self, id: Uuid) -> Result<(), ConnectionError> {
        let request = self.pending.remove(&id).ok_or(ConnectionError::NotFound)?;

        if !matches!(request.state, ConnectionState::Approved) {
            return Err(ConnectionError::InvalidState);
        }

        self.active.insert(id, Instant::now());
        Ok(())
    }

    pub fn close_connection(&mut self, id: Uuid) -> Result<(), ConnectionError> {
        if self.active.remove(&id).is_some() {
            Ok(())
        } else {
            Err(ConnectionError::NotFound)
        }
    }
}
