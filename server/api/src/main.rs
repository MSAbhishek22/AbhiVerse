use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "abhiverse_api=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let addr = abhiverse_api::bind_addr();
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(
        "ABHIVERSE API {} listening on http://{}",
        env!("CARGO_PKG_VERSION"),
        addr
    );
    axum::serve(listener, abhiverse_api::build_app())
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
