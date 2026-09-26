use redash_server::build_router;
use redash_server::state::AppState;
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let state = AppState::new();

    let mut port = 8080;
    let mut host = "127.0.0.1".to_string();

    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--port" && i + 1 < args.len() {
            port = args[i + 1].parse().unwrap_or(8080);
            i += 2;
        } else if args[i] == "--host" && i + 1 < args.len() {
            host = args[i + 1].clone();
            i += 2;
        } else {
            i += 1;
        }
    }

    let app = build_router(state);
    let addr: SocketAddr = format!("{}:{}", host, port).parse()?;

    println!();
    println!("  ┌─────────────────────────────────────────────────────────────┐");
    println!("  │                   ⚡ ReDash Web Gateway ⚡                   │");
    println!("  │                                                             │");
    println!("  │  Web Interface:    http://{:<33} │", format!("{}:{}", host, port));
    println!("  │  WebSocket Stream: ws://{:<35} │", format!("{}:{}/ws", host, port));
    println!("  │  REST API:         http://{:<33} │", format!("{}:{}/api", host, port));
    println!("  │  Status:           Online (120 FPS High-Performance Web)   │");
    println!("  └─────────────────────────────────────────────────────────────┘");
    println!();

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
