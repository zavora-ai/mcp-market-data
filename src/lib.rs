//! Market Data MCP Server library surface.
//!
//! A market-data platform: instruments, quotes & historical bars, analytics
//! (returns/volatility/correlation/moving averages), term-structure curves with
//! interpolation, FX conversion, benchmarks/indices, watchlists, price alerts,
//! forecasting, and gated official-mark publication — over an audit trail.

pub mod analytics;
pub mod live;
pub mod server;
pub mod store;
pub mod types;
