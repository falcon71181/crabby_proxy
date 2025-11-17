use crate::connection::ConnectionManager;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio_rustls::TlsAcceptor;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub connections: Arc<RwLock<ConnectionManager>>,
    pub notify_tx: mpsc::Sender<Uuid>,
    pub require_creds: bool,
    pub username: Option<String>,
    pub password: Option<String>,
    pub tls_acceptor: Option<TlsAcceptor>, // Add TLS acceptor for HTTPS
}

impl AppState {
    pub fn new(
        notify_tx: mpsc::Sender<Uuid>,
        require_creds: bool,
        username: Option<String>,
        password: Option<String>,
        tls_acceptor: Option<TlsAcceptor>,
    ) -> Self {
        Self {
            connections: Arc::new(RwLock::new(ConnectionManager::new())),
            notify_tx,
            require_creds: require_creds,
            username,
            password,
            tls_acceptor,
        }
    }
}
