use crate::connection::ConnectionManager;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct AppState {
    pub connections: Arc<RwLock<ConnectionManager>>,
    pub notify_tx: mpsc::Sender<Uuid>,
}

impl AppState {
    pub fn new(notify_tx: mpsc::Sender<Uuid>) -> Self {
        Self {
            connections: Arc::new(RwLock::new(ConnectionManager::new())),
            notify_tx,
        }
    }
}
