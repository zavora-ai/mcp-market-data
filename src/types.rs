//! Market-data platform domain model.
//!
//! Broad market/reference-data platform: instruments across asset classes,
//! real-time quotes and historical OHLC bars, analytics (returns, volatility,
//! correlation, moving averages), term-structure curves (yield/forward) with
//! interpolation, FX conversion, benchmarks/indices, watchlists, price alerts,
//! and simple forecasting. The named agents (Treasury Liquidity Analyst, Demand
//! Forecast Agent, Renewable Dispatch Assistant) are clients of this platform.

use chrono::{DateTime, NaiveDate, Utc};
use rmcp::schemars;
use serde::{Deserialize, Serialize};

// ─── instruments ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssetClass {
    Equity,
    Bond,
    Fx,
    Commodity,
    Rate,
    /// Power/energy (MWh), gas, etc.
    Energy,
    Index,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Instrument {
    pub id: String,
    /// Ticker / symbol, e.g. "AAPL", "EURUSD", "UST10Y", "PWR_BASE".
    pub symbol: String,
    pub name: String,
    pub asset_class: AssetClass,
    pub currency: String,
    /// Quote unit, e.g. "USD", "%", "USD/MWh".
    pub unit: String,
    pub created_at: DateTime<Utc>,
}

// ─── quotes & bars ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Quote {
    pub instrument_id: String,
    pub bid: f64,
    pub ask: f64,
    pub last: f64,
    pub at: DateTime<Utc>,
}

/// Daily OHLC bar (the historian unit).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Bar {
    pub instrument_id: String,
    pub date: NaiveDate,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

// ─── curves (term structure) ─────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CurveKind {
    /// Interest-rate yield curve.
    Yield,
    /// Forward price curve (energy/commodity).
    Forward,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CurvePoint {
    /// Tenor in years (e.g. 0.25, 1.0, 10.0) or delivery horizon.
    pub tenor: f64,
    pub value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Curve {
    pub id: String,
    pub name: String,
    pub kind: CurveKind,
    pub currency: String,
    pub unit: String,
    /// Sorted by tenor ascending.
    pub points: Vec<CurvePoint>,
    pub as_of: NaiveDate,
}

// ─── benchmarks / indices ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Benchmark {
    pub id: String,
    pub name: String,
    /// Member instrument ids (equal-weighted for the reference index).
    pub members: Vec<String>,
    pub created_at: DateTime<Utc>,
}

// ─── watchlists ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Watchlist {
    pub id: String,
    pub name: String,
    pub owner: String,
    pub instrument_ids: Vec<String>,
    pub created_at: DateTime<Utc>,
}

// ─── alerts ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AlertCondition {
    Above,
    Below,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AlertStatus {
    Armed,
    Triggered,
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Alert {
    pub id: String,
    pub instrument_id: String,
    pub condition: AlertCondition,
    pub threshold: f64,
    pub status: AlertStatus,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub triggered_at: Option<DateTime<Utc>>,
    pub triggered_value: Option<f64>,
}

// ─── official marks (the gated write) ────────────────────────────────────────

/// An official/closing mark published for downstream valuation & risk. Publishing
/// one has real-world weight (it feeds P&L, NAV, collateral) — hence gated.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OfficialMark {
    pub id: String,
    pub instrument_id: String,
    pub price: f64,
    pub currency: String,
    pub as_of: NaiveDate,
    pub source: String,
    pub published_by: String,
    pub published_at: DateTime<Utc>,
}

// ─── audit trail ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AuditEntry {
    pub at: DateTime<Utc>,
    pub actor: String,
    pub action: String,
    pub detail: String,
}
