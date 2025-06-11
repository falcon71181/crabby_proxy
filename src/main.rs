mod app_state;
mod config;
mod connection;
mod error;
mod proxy;
mod utils;
use crate::app_state::AppState;
use crate::proxy::listener::run_proxy_server;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let (notify_tx, _) = mpsc::channel(100);
    let state = AppState::new(notify_tx);

    let proxy_handle = tokio::spawn(run_proxy_server(
        state.clone(),
        "0.0.0.0:8080".parse().unwrap(),
    ));

    // let admin_handle = tokio::spawn(run_admin_server(
    //     state.clone(),
    //     "0.0.0.0:8081".parse().unwrap(),
    // ));

    // Wait for servers
    let _ = tokio::join!(proxy_handle);
}
