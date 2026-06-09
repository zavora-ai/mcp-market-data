//! MCP tool surface for the market-data platform.
//!
//! This is a read-heavy reference/analytics platform: quotes, history, analytics,
//! curves, FX, benchmarks, watchlists, and forecasting are `read_only`. Quote/bar
//! ingest, curve/benchmark/watchlist/alert creation are `internal_write`.
//!
//! The one write with real-world weight is `publish_mark` — an official/closing
//! mark feeds downstream valuation, P&L and collateral — so it is `external_write`
//! + `requires_approval`.

use crate::store::MarketDataStore;
use crate::types::*;
use adk_mcp_sdk::{HealthCheck, HealthStatus};
use chrono::NaiveDate;
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::Deserialize;
use std::sync::Arc;

fn dactor() -> String { "agent".into() }
fn date(s: &Option<String>) -> Option<NaiveDate> { s.as_ref().and_then(|x| NaiveDate::parse_from_str(x, "%Y-%m-%d").ok()) }
fn today() -> NaiveDate { chrono::Utc::now().date_naive() }
fn dusd() -> String { "USD".into() }

// ─── inputs ───────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateInstrumentInput { pub symbol: String, #[serde(default)] pub name: String, pub asset_class: AssetClass, #[serde(default = "dusd")] pub currency: String, #[serde(default = "dusd")] pub unit: String, #[serde(default = "dactor")] pub actor: String }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ResolveInput { pub instrument: String }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListInstrumentsInput { pub asset_class: Option<AssetClass> }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SetQuoteInput { pub instrument_id: String, pub bid: f64, pub ask: f64, pub last: f64, #[serde(default = "dactor")] pub actor: String }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct InstrumentIdInput { pub instrument_id: String }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AddBarInput { pub instrument_id: String, pub date: String, pub open: f64, pub high: f64, pub low: f64, pub close: f64, #[serde(default)] pub volume: f64, #[serde(default = "dactor")] pub actor: String }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct HistoryInput { pub instrument_id: String, pub limit: Option<usize> }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MovingAverageInput { pub instrument_id: String, #[serde(default = "dwin")] pub window: usize }
fn dwin() -> usize { 5 }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CorrelationInput { pub a: String, pub b: String }

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CurvePointInput { pub tenor: f64, pub value: f64 }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateCurveInput { pub name: String, pub kind: CurveKind, #[serde(default = "dusd")] pub currency: String, #[serde(default)] pub unit: String, pub points: Vec<CurvePointInput>, pub as_of: Option<String>, #[serde(default = "dactor")] pub actor: String }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListCurvesInput { pub kind: Option<CurveKind> }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct InterpolateInput { pub curve_id: String, pub tenor: f64 }

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FxConvertInput { #[serde(default = "done")] pub amount: f64, pub from: String, pub to: String }
fn done() -> f64 { 1.0 }

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateBenchmarkInput { pub name: String, pub members: Vec<String>, #[serde(default = "dactor")] pub actor: String }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct BenchmarkIdInput { pub benchmark_id: String }

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateWatchlistInput { pub name: String, #[serde(default = "dactor")] pub owner: String, pub instrument_ids: Vec<String>, #[serde(default = "dactor")] pub actor: String }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WatchlistIdInput { pub watchlist_id: String }

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateAlertInput { pub instrument_id: String, pub condition: AlertCondition, pub threshold: f64, #[serde(default = "dactor")] pub created_by: String }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListAlertsInput { pub status: Option<AlertStatus> }

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ForecastInput { pub instrument_id: String, #[serde(default = "dhoriz")] pub horizon: usize }
fn dhoriz() -> usize { 7 }

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PublishMarkInput { pub instrument_id: String, pub price: f64, pub as_of: Option<String>, #[serde(default = "dmark")] pub source: String, #[serde(default = "dactor")] pub published_by: String }
fn dmark() -> String { "official-close".into() }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListMarksInput { pub instrument_id: Option<String>, #[serde(default = "dlimit")] pub limit: usize }
fn dlimit() -> usize { 50 }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AuditLogInput { #[serde(default = "dlimit")] pub limit: usize }

// ─── server ───────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct MarketDataServer { pub store: Arc<MarketDataStore> }

#[tool_router(server_handler)]
impl MarketDataServer {
    // instruments
    #[tool(description = "Define an instrument (equity/bond/fx/commodity/rate/energy/index).")]
    fn create_instrument(&self, Parameters(i): Parameters<CreateInstrumentInput>) -> String {
        let x = self.store.create_instrument(&i.symbol, &i.name, i.asset_class, &i.currency, &i.unit, &i.actor);
        serde_json::to_string_pretty(&x).unwrap()
    }

    #[tool(description = "Resolve an instrument by id or symbol.")]
    fn get_instrument(&self, Parameters(i): Parameters<ResolveInput>) -> String {
        match self.store.resolve(&i.instrument) {
            Some(x) => serde_json::to_string_pretty(&x).unwrap(), None => format!("Instrument not found: {}", i.instrument) }
    }

    #[tool(description = "List instruments, optionally by asset class.")]
    fn list_instruments(&self, Parameters(i): Parameters<ListInstrumentsInput>) -> String {
        let v = self.store.list_instruments(i.asset_class);
        serde_json::to_string_pretty(&serde_json::json!({"count": v.len(), "instruments": v})).unwrap()
    }

    // quotes & bars
    #[tool(description = "Set the latest quote (bid/ask/last) for an instrument; evaluates armed price alerts.")]
    fn set_quote(&self, Parameters(i): Parameters<SetQuoteInput>) -> String {
        match self.store.set_quote(&i.instrument_id, i.bid, i.ask, i.last, &i.actor) {
            Ok(q) => serde_json::to_string_pretty(&q).unwrap(), Err(e) => format!("Error: {e}") }
    }

    #[tool(description = "Get the latest quote for an instrument.")]
    fn get_quote(&self, Parameters(i): Parameters<InstrumentIdInput>) -> String {
        match self.store.get_quote(&i.instrument_id) {
            Some(q) => serde_json::to_string_pretty(&q).unwrap(), None => format!("No quote for: {}", i.instrument_id) }
    }

    #[tool(description = "Add/replace a daily OHLC bar for an instrument.")]
    fn add_bar(&self, Parameters(i): Parameters<AddBarInput>) -> String {
        let Some(d) = date(&Some(i.date.clone())) else { return "Error: date must be YYYY-MM-DD".into() };
        match self.store.add_bar(&i.instrument_id, d, i.open, i.high, i.low, i.close, i.volume, &i.actor) {
            Ok(b) => serde_json::to_string_pretty(&b).unwrap(), Err(e) => format!("Error: {e}") }
    }

    #[tool(description = "Historical OHLC bars for an instrument (optionally the most recent `limit`).")]
    fn history(&self, Parameters(i): Parameters<HistoryInput>) -> String {
        let v = self.store.history(&i.instrument_id, i.limit);
        serde_json::to_string_pretty(&serde_json::json!({"count": v.len(), "bars": v})).unwrap()
    }

    // analytics
    #[tool(description = "Price analytics for an instrument: last/min/max/mean, period return, daily and annualized volatility.")]
    fn analytics(&self, Parameters(i): Parameters<InstrumentIdInput>) -> String {
        match self.store.analytics(&i.instrument_id) {
            Some(v) => serde_json::to_string_pretty(&v).unwrap(), None => format!("Instrument not found: {}", i.instrument_id) }
    }

    #[tool(description = "Simple moving average of closes over a window.")]
    fn moving_average(&self, Parameters(i): Parameters<MovingAverageInput>) -> String {
        match self.store.moving_average(&i.instrument_id, i.window) {
            Some(v) => serde_json::to_string_pretty(&v).unwrap(), None => format!("Instrument not found: {}", i.instrument_id) }
    }

    #[tool(description = "Pearson correlation of daily returns between two instruments (diversification/hedge analysis).")]
    fn correlation(&self, Parameters(i): Parameters<CorrelationInput>) -> String {
        match self.store.correlation(&i.a, &i.b) {
            Some(v) => serde_json::to_string_pretty(&v).unwrap(), None => "Error: instruments not found".into() }
    }

    // curves
    #[tool(description = "Create a term-structure curve (yield or forward) from tenor/value points.")]
    fn create_curve(&self, Parameters(i): Parameters<CreateCurveInput>) -> String {
        let pts = i.points.into_iter().map(|p| CurvePoint { tenor: p.tenor, value: p.value }).collect();
        let c = self.store.create_curve(&i.name, i.kind, &i.currency, &i.unit, pts, date(&i.as_of).unwrap_or_else(today), &i.actor);
        serde_json::to_string_pretty(&c).unwrap()
    }

    #[tool(description = "List curves, optionally by kind (yield/forward).")]
    fn list_curves(&self, Parameters(i): Parameters<ListCurvesInput>) -> String {
        let v = self.store.list_curves(i.kind);
        serde_json::to_string_pretty(&serde_json::json!({"count": v.len(), "curves": v})).unwrap()
    }

    #[tool(description = "Linearly interpolate a curve's value at a tenor (flat beyond the ends).")]
    fn interpolate_curve(&self, Parameters(i): Parameters<InterpolateInput>) -> String {
        match self.store.interpolate_curve(&i.curve_id, i.tenor) {
            Some(v) => serde_json::to_string_pretty(&v).unwrap(), None => format!("Curve not found: {}", i.curve_id) }
    }

    // FX
    #[tool(description = "Convert an amount between currencies using FX quotes (direct, inverse, or triangulated via USD).")]
    fn fx_convert(&self, Parameters(i): Parameters<FxConvertInput>) -> String {
        match self.store.fx_convert(i.amount, &i.from, &i.to) {
            Ok(v) => serde_json::to_string_pretty(&v).unwrap(), Err(e) => format!("Error: {e}") }
    }

    // benchmarks
    #[tool(description = "Create an equal-weighted benchmark/index from member instruments.")]
    fn create_benchmark(&self, Parameters(i): Parameters<CreateBenchmarkInput>) -> String {
        let b = self.store.create_benchmark(&i.name, i.members, &i.actor);
        serde_json::to_string_pretty(&b).unwrap()
    }

    #[tool(description = "Compute an equal-weighted benchmark level from member last prices, with contributions.")]
    fn benchmark_level(&self, Parameters(i): Parameters<BenchmarkIdInput>) -> String {
        match self.store.benchmark_level(&i.benchmark_id) {
            Some(v) => serde_json::to_string_pretty(&v).unwrap(), None => format!("Benchmark not found: {}", i.benchmark_id) }
    }

    // watchlists
    #[tool(description = "Create a watchlist of instruments.")]
    fn create_watchlist(&self, Parameters(i): Parameters<CreateWatchlistInput>) -> String {
        let w = self.store.create_watchlist(&i.name, &i.owner, i.instrument_ids, &i.actor);
        serde_json::to_string_pretty(&w).unwrap()
    }

    #[tool(description = "Snapshot a watchlist with each instrument's latest quote.")]
    fn watchlist_quotes(&self, Parameters(i): Parameters<WatchlistIdInput>) -> String {
        match self.store.watchlist_quotes(&i.watchlist_id) {
            Some(v) => serde_json::to_string_pretty(&v).unwrap(), None => format!("Watchlist not found: {}", i.watchlist_id) }
    }

    // alerts
    #[tool(description = "Create a price alert (above/below threshold). Evaluated whenever a new quote arrives.")]
    fn create_alert(&self, Parameters(i): Parameters<CreateAlertInput>) -> String {
        match self.store.create_alert(&i.instrument_id, i.condition, i.threshold, &i.created_by) {
            Ok(a) => serde_json::to_string_pretty(&a).unwrap(), Err(e) => format!("Error: {e}") }
    }

    #[tool(description = "List price alerts, optionally by status (armed/triggered/disabled).")]
    fn list_alerts(&self, Parameters(i): Parameters<ListAlertsInput>) -> String {
        let v = self.store.list_alerts(i.status);
        serde_json::to_string_pretty(&serde_json::json!({"count": v.len(), "alerts": v})).unwrap()
    }

    // forecasting
    #[tool(description = "Forecast future values via linear-drift on the close series. Powers the Demand Forecast & Renewable Dispatch agents.")]
    fn forecast(&self, Parameters(i): Parameters<ForecastInput>) -> String {
        match self.store.forecast(&i.instrument_id, i.horizon) {
            Some(v) => serde_json::to_string_pretty(&v).unwrap(), None => format!("Instrument not found: {}", i.instrument_id) }
    }

    // official marks (gated)
    #[tool(description = "Publish an OFFICIAL mark/closing price for an instrument. This feeds downstream valuation, P&L and collateral — external write, gated.")]
    fn publish_mark(&self, Parameters(i): Parameters<PublishMarkInput>) -> String {
        match self.store.publish_mark(&i.instrument_id, i.price, date(&i.as_of).unwrap_or_else(today), &i.source, &i.published_by) {
            Ok(m) => serde_json::to_string_pretty(&m).unwrap(), Err(e) => format!("Error: {e}") }
    }

    #[tool(description = "Get the latest official mark for an instrument.")]
    fn latest_mark(&self, Parameters(i): Parameters<InstrumentIdInput>) -> String {
        match self.store.latest_mark(&i.instrument_id) {
            Some(m) => serde_json::to_string_pretty(&m).unwrap(), None => format!("No mark for: {}", i.instrument_id) }
    }

    #[tool(description = "List published official marks, optionally for one instrument.")]
    fn list_marks(&self, Parameters(i): Parameters<ListMarksInput>) -> String {
        let v = self.store.list_marks(i.instrument_id.as_deref(), i.limit);
        serde_json::to_string_pretty(&serde_json::json!({"count": v.len(), "marks": v})).unwrap()
    }

    #[tool(description = "Recent audit-trail entries (most recent first).")]
    fn audit_log(&self, Parameters(i): Parameters<AuditLogInput>) -> String {
        let v = self.store.audit_log(i.limit);
        serde_json::to_string_pretty(&serde_json::json!({"count": v.len(), "entries": v})).unwrap()
    }
}

#[async_trait::async_trait]
impl HealthCheck for MarketDataServer {
    async fn check_health(&self) -> HealthStatus {
        HealthStatus { healthy: true, message: Some("operational".into()), latency_ms: Some(1) }
    }
}
