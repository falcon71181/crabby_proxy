mod app_state;
mod config;
mod connection;
mod error;
mod proxy;
mod stream;
mod tunnel;
mod utils;

use crate::app_state::AppState;
use crate::proxy::listener::run_proxy_server;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();
    let arguments = std::env::args().collect::<Vec<String>>();

    let (notify_tx, _) = mpsc::channel(100);
    let no_creds = arguments.iter().any(|arg| arg == "--no-creds");
    let require_creds = !no_creds;

    let mut username: Option<String> = None;
    let mut password: Option<String> = None;

    if require_creds {
        // Parsing --username <value> and --password <value>
        // TODO: add and use dependency clap
        for i in 0..arguments.len() {
            match arguments[i].as_str() {
                "--username" if i + 1 < arguments.len() => {
                    username = Some(arguments[i + 1].clone());
                }
                "--password" if i + 1 < arguments.len() => {
                    password = Some(arguments[i + 1].clone());
                }
                _ => {}
            }
        }

        if username.is_none() || password.is_none() {
            tracing::error!("--username and --password are required unless you use --no-creds");
            std::process::exit(1);
        }
    }
    let state = AppState::new(notify_tx, require_creds, username, password);

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
