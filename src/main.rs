use mcp_market_data::live::{Backend, LiveClient};
use mcp_market_data::server::MarketDataServer;
use mcp_market_data::store::MarketDataStore;
use rmcp::{ServiceExt, transport::stdio};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env().add_directive("info".parse().unwrap()),
        )
        .init();
    let backend = Backend::from_env();
    tracing::info!("market-data backend: {}", backend.label());
    let store = Arc::new(MarketDataStore::new());
    let server = MarketDataServer { store, backend, live: Arc::new(LiveClient::new()) };
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
