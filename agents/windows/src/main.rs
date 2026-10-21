use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("abhiverse-agent {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    // --once: print a machine-readable status line and exit.
    // Used by smoke tests and installer health checks.
    if args.iter().any(|a| a == "--once") {
        println!(
            "{}",
            serde_json::to_string(&abhiverse_agent_windows::agent_info())?
        );
        return Ok(());
    }

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let info = abhiverse_agent_windows::agent_info();
    tracing::info!(
        "ABHIVERSE agent {} starting (platform: {}, protocol: {})",
        info.version,
        info.platform,
        info.protocol_version
    );
    tracing::info!("Task 001 build: no device capabilities exposed yet. Press Ctrl+C to stop.");

    tokio::signal::ctrl_c().await?;
    tracing::info!("Shutdown signal received, exiting cleanly.");
    Ok(())
}
