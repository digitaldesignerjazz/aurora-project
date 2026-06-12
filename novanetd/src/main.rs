//! NovaNet v0.1 Daemon (novanetd)
//! Sovereign, privacy-first mesh networking node for the Aurora Project.
//!
//! See specs/novanet-v0.1-technical-specification.md and
//! specs/novanet-wire-protocol-v0.1.md for full protocol definition.

use clap::Parser;
use std::path::PathBuf;
use tracing::{info, warn, error};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod config;
mod node;
mod crypto;
mod routing;
mod discovery;
mod bridge;

use config::NovanetConfig;
use node::NovanetNode;

#[derive(Parser, Debug)]
#[command(name = "novanetd", version, about = "NovaNet v0.1 Mesh Node Daemon")]
struct Args {
    /// Path to configuration file (TOML)
    #[arg(short, long, default_value = "/etc/novanet/novanet.toml")]
    config: PathBuf,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Initialize tracing
    let filter = if args.verbose {
        "novanetd=debug,info"
    } else {
        "novanetd=info,warn,error"
    };
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(filter))
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("🌐 NovaNet v0.1 daemon starting...");
    info!("Config path: {:?}", args.config);

    // Load configuration
    let cfg = NovanetConfig::load(&args.config).await?;
    info!("Node identity loaded: {}", cfg.node.name);
    info!("Listening on: {:?}", cfg.peering.listen);

    // Initialize core node
    let mut node = NovanetNode::new(cfg).await?;

    // Start discovery, peering, routing, bridge subsystems
    node.start().await?;

    // Block forever (or until shutdown signal)
    tokio::signal::ctrl_c().await?;
    info!("Shutdown signal received. Gracefully stopping...");

    node.shutdown().await?;
    info!("NovaNet node stopped cleanly. Goodbye.");

    Ok(())
}