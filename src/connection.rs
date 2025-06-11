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

#[derive(Debug)]
pub struct ConnectionRequest {
    pub id: Uuid,
    pub client_addr: SocketAddr,
    pub target: ProxyTarget,
    pub state: ConnectionState,
    pub decision_tx: Option<oneshot::Sender<bool>>,
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
        target: ProxyTarget,
        decision_tx: Option<oneshot::Sender<bool>>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            client_addr,
            target,
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
            let _ = tx.send(true); // Ignore send errors
        }

        Ok(())
    }

    pub fn reject_connection(&mut self, id: Uuid) -> Result<(), ConnectionError> {
        let mut request = self.pending.remove(&id).ok_or(ConnectionError::NotFound)?;

        if !matches!(request.state, ConnectionState::Pending) {
            return Err(ConnectionError::InvalidState);
        }

        request.state = ConnectionState::Rejected;

        if let Some(tx) = request.decision_tx.take() {
            let _ = tx.send(false);
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
