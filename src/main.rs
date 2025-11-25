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
use clap::Parser;
use tokio::sync::mpsc;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long)]
    no_creds: bool,

    #[arg(short = 'u', long, required_unless_present = "no_creds")]
    username: Option<String>,

    #[arg(short = 'p', long, required_unless_present = "no_creds")]
    password: Option<String>,

    #[arg(short = 'P', long, default_value = "8080")]
    proxy_port: u32,

    #[arg(short = 't', long)]
    tls_certificate: Option<String>,

    #[arg(short = 'k', long, requires = "tls_certificate")]
    tls_private_key: Option<String>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();
    let (notify_tx, _) = mpsc::channel(100);

    let args = Args::parse();

    let state = AppState::new(
        notify_tx,
        !args.no_creds,
        args.username,
        args.password,
        None,
    );

    let addr: String = format!("0.0.0.0:{}", &args.proxy_port);
    tracing::info!("Starting Proxy Server at: {}", &addr);

    let proxy_handle = tokio::spawn(run_proxy_server(state.clone(), addr.parse().unwrap()));

    // TODO: make api admin flow
    // let admin_handle = tokio::spawn(run_admin_server(
    //     state.clone(),
    //     "0.0.0.0:8081".parse().unwrap(),
    // ));

    // Wait for servers
    let _ = tokio::join!(proxy_handle);
}
