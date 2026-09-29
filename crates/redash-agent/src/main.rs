use redash_agent::{AgentClient, AgentConfig};
use std::env;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let mut hub_url = env::var("REDASH_HUB_URL")
        .unwrap_or_else(|_| "ws://127.0.0.1:8080/v1/agent/ws".to_string());
    let mut node_id = env::var("REDASH_NODE_ID").ok().filter(|s| !s.trim().is_empty()).unwrap_or_else(|| {
        let host = gethostname::gethostname().to_string_lossy().into_owned();
        format!("node-{}", host)
    });
    let mut auth_token =
        env::var("REDASH_AUTH_TOKEN").unwrap_or_default();
    let mut identity_private_key = env::var("REDASH_IDENTITY_KEY").ok();
    let mut trusted_key = env::var("REDASH_TRUSTED_KEY").ok();
    let mut interval_secs = 3u64;

    let args: Vec<String> = env::args().collect();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--hub" | "-h" if i + 1 < args.len() => {
                hub_url = args[i + 1].clone();
                i += 2;
            }
            "--node-id" | "-n" if i + 1 < args.len() => {
                node_id = args[i + 1].clone();
                i += 2;
            }
            "--token" | "-t" if i + 1 < args.len() => {
                auth_token = args[i + 1].clone();
                i += 2;
            }
            "--trusted-key" | "-k" if i + 1 < args.len() => {
                trusted_key = Some(args[i + 1].clone());
                i += 2;
            }
            "--identity-key" if i + 1 < args.len() => {
                identity_private_key = Some(args[i + 1].clone()); i += 2;
            }
            "--interval" | "-i" if i + 1 < args.len() => {
                interval_secs = args[i + 1].parse().unwrap_or(3);
                i += 2;
            }
            "--help" => {
                println!(
                    "ReDash Next-Gen Agent - Lightweight Zero-Trust Host Probe & Remediation Agent"
                );
                println!();
                println!("Usage: redash-agent [OPTIONS]");
                println!();
                println!("Options:");
                println!(
                    "  -h, --hub <URL>          Hub WebSocket URL (default: ws://127.0.0.1:8080/v1/agent/ws)"
                );
                println!(
                    "  -n, --node-id <ID>       Unique Node Identifier (default: hostname-based)"
                );
                println!(
                    "  -t, --token <TOKEN>      Provisioned per-node Authentication Token (required)"
                );
                println!(
                    "  -k, --trusted-key <HEX>  ED25519 Public Key for cryptographic zero-trust validation"
                );
                println!(
                    "  -i, --interval <SECS>    Telemetry reporting interval in seconds (default: 3)"
                );
                println!("      --identity-key <HEX> Provisioned Agent Ed25519 private identity key");
                println!("      --help               Display this help text");
                return Ok(());
            }
            _ => {
                i += 1;
            }
        }
    }

    println!();
    println!("  ┌─────────────────────────────────────────────────────────────┐");
    println!("  │                ⚡ ReDash Next-Gen Agent ⚡                   │");
    println!("  │                                                             │");
    println!("  │  Node ID:     {:<45} │", node_id);
    println!("  │  Hub Target:  {:<45} │", hub_url);
    println!(
        "  │  Zero-Trust:  {:<45} │",
        if trusted_key.is_some() {
            "Enforced (ED25519)"
        } else {
            "Telemetry only (commands disabled)"
        }
    );
    println!(
        "  │  Interval:    {:<45} │",
        format!("{} seconds", interval_secs)
    );
    println!("  └─────────────────────────────────────────────────────────────┘");
    println!();

    let config = AgentConfig {
        hub_url,
        node_id,
        auth_token,
        trusted_public_key: trusted_key,
        telemetry_interval_secs: interval_secs,
        identity_private_key,
    };

    let client = AgentClient::new(config)?;
    client.run_forever().await;

    Ok(())
}
