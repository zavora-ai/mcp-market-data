//! In-memory market-data store with seeded data and analytics engines.
//!
//! Thread-safe via per-collection `Mutex`. IDs come from a monotonic sequence
//! (`PREFIX-{n}` from 1000). Publishing an official mark appends to an audit
//! trail. Analytics (returns, volatility, correlation, moving averages, curve
//! interpolation, FX cross, forecasting) are computed on read from the bar set.

use crate::types::*;
use chrono::{Duration, NaiveDate, Utc};
use std::collections::HashMap;
use std::sync::Mutex;

pub struct MarketDataStore {
    instruments: Mutex<HashMap<String, Instrument>>,
    quotes: Mutex<HashMap<String, Quote>>,
    bars: Mutex<HashMap<String, Vec<Bar>>>, // instrument_id -> sorted bars
    curves: Mutex<HashMap<String, Curve>>,
    benchmarks: Mutex<HashMap<String, Benchmark>>,
    watchlists: Mutex<HashMap<String, Watchlist>>,
    alerts: Mutex<HashMap<String, Alert>>,
    marks: Mutex<Vec<OfficialMark>>,
    audit_log: Mutex<Vec<AuditEntry>>,
    seq: Mutex<u64>,
}

impl Default for MarketDataStore {
    fn default() -> Self {
        Self::new()
    }
}

impl MarketDataStore {
    pub fn new() -> Self {
        let s = MarketDataStore {
            instruments: Mutex::new(HashMap::new()),
            quotes: Mutex::new(HashMap::new()),
            bars: Mutex::new(HashMap::new()),
            curves: Mutex::new(HashMap::new()),
            benchmarks: Mutex::new(HashMap::new()),
            watchlists: Mutex::new(HashMap::new()),
            alerts: Mutex::new(HashMap::new()),
            marks: Mutex::new(Vec::new()),
            audit_log: Mutex::new(Vec::new()),
            seq: Mutex::new(1000),
        };
        s.seed();
        s
    }

    fn next(&self, prefix: &str) -> String {
        let mut n = self.seq.lock().unwrap();
        *n += 1;
        format!("{prefix}-{n}")
    }

    fn audit(&self, actor: &str, action: &str, detail: impl Into<String>) {
        self.audit_log.lock().unwrap().push(AuditEntry { at: Utc::now(), actor: actor.to_string(), action: action.to_string(), detail: detail.into() });
    }

    // ─── instruments ─────────────────────────────────────────────────────

    pub fn create_instrument(&self, symbol: &str, name: &str, asset_class: AssetClass, currency: &str, unit: &str, actor: &str) -> Instrument {
        let i = Instrument { id: self.next("INS"), symbol: symbol.to_string(), name: name.to_string(), asset_class, currency: currency.to_string(), unit: unit.to_string(), created_at: Utc::now() };
        self.instruments.lock().unwrap().insert(i.id.clone(), i.clone());
        self.audit(actor, "create_instrument", i.symbol.clone());
        i
    }

    pub fn get_instrument(&self, id: &str) -> Option<Instrument> {
        self.instruments.lock().unwrap().get(id).cloned()
    }

    /// Resolve an instrument by id or by symbol (case-insensitive).
    pub fn resolve(&self, id_or_symbol: &str) -> Option<Instrument> {
        let ins = self.instruments.lock().unwrap();
        ins.get(id_or_symbol).cloned().or_else(|| ins.values().find(|i| i.symbol.eq_ignore_ascii_case(id_or_symbol)).cloned())
    }

    pub fn list_instruments(&self, asset_class: Option<AssetClass>) -> Vec<Instrument> {
        let mut v: Vec<Instrument> = self.instruments.lock().unwrap().values().filter(|i| asset_class.is_none_or(|c| i.asset_class == c)).cloned().collect();
        v.sort_by(|a, b| a.symbol.cmp(&b.symbol));
        v
    }

    // ─── quotes & bars ───────────────────────────────────────────────────

    pub fn set_quote(&self, instrument_id: &str, bid: f64, ask: f64, last: f64, actor: &str) -> std::result::Result<Quote, String> {
        if self.get_instrument(instrument_id).is_none() { return Err(format!("Instrument not found: {instrument_id}")); }
        let q = Quote { instrument_id: instrument_id.to_string(), bid, ask, last, at: Utc::now() };
        self.quotes.lock().unwrap().insert(instrument_id.to_string(), q.clone());
        // Evaluate any armed alerts on this instrument against the new last price.
        self.evaluate_alerts(instrument_id, last);
        self.audit(actor, "set_quote", format!("{instrument_id} last={last}"));
        Ok(q)
    }

    pub fn get_quote(&self, instrument_id: &str) -> Option<Quote> {
        self.quotes.lock().unwrap().get(instrument_id).cloned()
    }

    pub fn add_bar(&self, instrument_id: &str, date: NaiveDate, open: f64, high: f64, low: f64, close: f64, volume: f64, actor: &str) -> std::result::Result<Bar, String> {
        if self.get_instrument(instrument_id).is_none() { return Err(format!("Instrument not found: {instrument_id}")); }
        let bar = Bar { instrument_id: instrument_id.to_string(), date, open, high, low, close, volume };
        let mut bars = self.bars.lock().unwrap();
        let series = bars.entry(instrument_id.to_string()).or_default();
        // Replace same-date bar, else insert sorted.
        series.retain(|b| b.date != date);
        series.push(bar.clone());
        series.sort_by(|a, b| a.date.cmp(&b.date));
        drop(bars);
        self.audit(actor, "add_bar", format!("{instrument_id} {date}"));
        Ok(bar)
    }

    /// Historical bars, optionally the most recent `limit` (chronological).
    pub fn history(&self, instrument_id: &str, limit: Option<usize>) -> Vec<Bar> {
        let bars = self.bars.lock().unwrap();
        let series = bars.get(instrument_id).cloned().unwrap_or_default();
        match limit {
            Some(n) if n < series.len() => series[series.len() - n..].to_vec(),
            _ => series,
        }
    }

    fn closes(&self, instrument_id: &str) -> Vec<f64> {
        self.bars.lock().unwrap().get(instrument_id).map(|s| s.iter().map(|b| b.close).collect()).unwrap_or_default()
    }

    // ─── analytics ─────────────────────────────────────────────────────────

    /// Summary stats over a price series: last, min, max, mean, simple & log
    /// returns (period), and annualized volatility of daily returns.
    pub fn analytics(&self, instrument_id: &str) -> Option<serde_json::Value> {
        let inst = self.get_instrument(instrument_id)?;
        let closes = self.closes(instrument_id);
        Some(crate::analytics::summarize("instrument_id", instrument_id, &inst.symbol, &closes))
    }

    /// Simple moving average of closes over `window` days (latest value + series).
    pub fn moving_average(&self, instrument_id: &str, window: usize) -> Option<serde_json::Value> {
        let closes = self.closes(instrument_id);
        match crate::analytics::moving_average(&closes, window) {
            Some(sma) => Some(serde_json::json!({"instrument_id": instrument_id, "window": window, "latest": sma.last().copied(), "series": sma})),
            None => Some(serde_json::json!({"instrument_id": instrument_id, "window": window, "note": "insufficient history"})),
        }
    }

    /// Pearson correlation of daily returns between two instruments over the
    /// overlapping window. For diversification / hedge analysis.
    pub fn correlation(&self, a: &str, b: &str) -> Option<serde_json::Value> {
        let ca = self.closes(a);
        let cb = self.closes(b);
        let n = ca.len().min(cb.len());
        if n < 3 { return Some(serde_json::json!({"a": a, "b": b, "note": "insufficient overlapping history"})); }
        let ra = crate::analytics::daily_returns(&ca[ca.len() - n..]);
        let rb = crate::analytics::daily_returns(&cb[cb.len() - n..]);
        let corr = crate::analytics::pearson(&ra, &rb);
        Some(serde_json::json!({"a": a, "b": b, "samples": ra.len(), "correlation": corr.map(crate::analytics::round4)}))
    }

    // ─── curves ────────────────────────────────────────────────────────────

    pub fn create_curve(&self, name: &str, kind: CurveKind, currency: &str, unit: &str, mut points: Vec<CurvePoint>, as_of: NaiveDate, actor: &str) -> Curve {
        points.sort_by(|a, b| a.tenor.partial_cmp(&b.tenor).unwrap());
        let c = Curve { id: self.next("CRV"), name: name.to_string(), kind, currency: currency.to_string(), unit: unit.to_string(), points, as_of };
        self.curves.lock().unwrap().insert(c.id.clone(), c.clone());
        self.audit(actor, "create_curve", c.id.clone());
        c
    }

    pub fn get_curve(&self, id: &str) -> Option<Curve> {
        self.curves.lock().unwrap().get(id).cloned()
    }

    pub fn list_curves(&self, kind: Option<CurveKind>) -> Vec<Curve> {
        let mut v: Vec<Curve> = self.curves.lock().unwrap().values().filter(|c| kind.is_none_or(|k| c.kind == k)).cloned().collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }

    /// Linearly interpolate a curve value at a tenor (flat-extrapolate beyond ends).
    pub fn interpolate_curve(&self, curve_id: &str, tenor: f64) -> Option<serde_json::Value> {
        let c = self.get_curve(curve_id)?;
        if c.points.is_empty() { return Some(serde_json::json!({"curve_id": curve_id, "tenor": tenor, "value": serde_json::Value::Null})); }
        let pts = &c.points;
        let value = if tenor <= pts[0].tenor {
            pts[0].value
        } else if tenor >= pts[pts.len() - 1].tenor {
            pts[pts.len() - 1].value
        } else {
            let mut v = pts[0].value;
            for w in pts.windows(2) {
                if tenor >= w[0].tenor && tenor <= w[1].tenor {
                    let frac = (tenor - w[0].tenor) / (w[1].tenor - w[0].tenor);
                    v = w[0].value + frac * (w[1].value - w[0].value);
                    break;
                }
            }
            v
        };
        Some(serde_json::json!({"curve_id": curve_id, "name": c.name, "tenor": tenor, "value": round4(value), "unit": c.unit}))
    }

    // ─── FX ────────────────────────────────────────────────────────────────

    /// Convert an amount between currencies using FX instrument quotes. Looks for
    /// a direct pair (BASEQUOTE), an inverse, or triangulates via USD.
    pub fn fx_convert(&self, amount: f64, from: &str, to: &str) -> std::result::Result<serde_json::Value, String> {
        let from = from.to_uppercase();
        let to = to.to_uppercase();
        if from == to { return Ok(serde_json::json!({"amount": amount, "from": from, "to": to, "rate": 1.0, "result": round4(amount)})); }
        let rate = self.fx_rate(&from, &to).ok_or_else(|| format!("no FX path {from}->{to}"))?;
        Ok(serde_json::json!({"amount": amount, "from": from, "to": to, "rate": round6(rate), "result": round4(amount * rate)}))
    }

    fn fx_rate(&self, from: &str, to: &str) -> Option<f64> {
        // direct or inverse
        if let Some(r) = self.pair_last(&format!("{from}{to}")) { return Some(r); }
        if let Some(r) = self.pair_last(&format!("{to}{from}")) { return Some(1.0 / r); }
        // triangulate via USD
        if from != "USD" && to != "USD" {
            let f_usd = self.fx_rate(from, "USD")?;
            let usd_t = self.fx_rate("USD", to)?;
            return Some(f_usd * usd_t);
        }
        None
    }

    fn pair_last(&self, symbol: &str) -> Option<f64> {
        let ins = self.instruments.lock().unwrap();
        let inst = ins.values().find(|i| i.asset_class == AssetClass::Fx && i.symbol.eq_ignore_ascii_case(symbol))?;
        let id = inst.id.clone();
        drop(ins);
        self.get_quote(&id).map(|q| q.last)
    }

    // ─── benchmarks / indices ────────────────────────────────────────────

    pub fn create_benchmark(&self, name: &str, members: Vec<String>, actor: &str) -> Benchmark {
        let b = Benchmark { id: self.next("BMK"), name: name.to_string(), members, created_at: Utc::now() };
        self.benchmarks.lock().unwrap().insert(b.id.clone(), b.clone());
        self.audit(actor, "create_benchmark", b.id.clone());
        b
    }

    /// Equal-weighted index level = mean of member last prices; also returns
    /// member contributions. A simple reference index.
    pub fn benchmark_level(&self, benchmark_id: &str) -> Option<serde_json::Value> {
        let b = self.benchmarks.lock().unwrap().get(benchmark_id).cloned()?;
        let mut members = Vec::new();
        let mut sum = 0.0; let mut count = 0;
        for m in &b.members {
            if let Some(q) = self.get_quote(m) {
                sum += q.last; count += 1;
                let sym = self.get_instrument(m).map(|i| i.symbol).unwrap_or_default();
                members.push(serde_json::json!({"instrument_id": m, "symbol": sym, "last": q.last}));
            }
        }
        let level = if count > 0 { sum / count as f64 } else { 0.0 };
        Some(serde_json::json!({"benchmark_id": benchmark_id, "name": b.name, "level": round4(level), "members_priced": count, "members": members}))
    }

    // ─── watchlists ──────────────────────────────────────────────────────

    pub fn create_watchlist(&self, name: &str, owner: &str, instrument_ids: Vec<String>, actor: &str) -> Watchlist {
        let w = Watchlist { id: self.next("WL"), name: name.to_string(), owner: owner.to_string(), instrument_ids, created_at: Utc::now() };
        self.watchlists.lock().unwrap().insert(w.id.clone(), w.clone());
        self.audit(actor, "create_watchlist", w.id.clone());
        w
    }

    /// Watchlist snapshot: each instrument with its latest quote.
    pub fn watchlist_quotes(&self, watchlist_id: &str) -> Option<serde_json::Value> {
        let w = self.watchlists.lock().unwrap().get(watchlist_id).cloned()?;
        let rows: Vec<serde_json::Value> = w.instrument_ids.iter().map(|id| {
            let sym = self.get_instrument(id).map(|i| i.symbol).unwrap_or_default();
            let q = self.get_quote(id);
            serde_json::json!({"instrument_id": id, "symbol": sym, "last": q.as_ref().map(|x| x.last), "bid": q.as_ref().map(|x| x.bid), "ask": q.as_ref().map(|x| x.ask)})
        }).collect();
        Some(serde_json::json!({"watchlist_id": watchlist_id, "name": w.name, "count": rows.len(), "quotes": rows}))
    }

    // ─── alerts ────────────────────────────────────────────────────────────

    pub fn create_alert(&self, instrument_id: &str, condition: AlertCondition, threshold: f64, created_by: &str) -> std::result::Result<Alert, String> {
        if self.get_instrument(instrument_id).is_none() { return Err(format!("Instrument not found: {instrument_id}")); }
        let a = Alert { id: self.next("ALR"), instrument_id: instrument_id.to_string(), condition, threshold, status: AlertStatus::Armed, created_by: created_by.to_string(), created_at: Utc::now(), triggered_at: None, triggered_value: None };
        self.alerts.lock().unwrap().insert(a.id.clone(), a.clone());
        self.audit(created_by, "create_alert", a.id.clone());
        Ok(a)
    }

    fn evaluate_alerts(&self, instrument_id: &str, last: f64) {
        let mut alerts = self.alerts.lock().unwrap();
        for a in alerts.values_mut().filter(|a| a.instrument_id == instrument_id && a.status == AlertStatus::Armed) {
            let hit = match a.condition { AlertCondition::Above => last > a.threshold, AlertCondition::Below => last < a.threshold };
            if hit {
                a.status = AlertStatus::Triggered;
                a.triggered_at = Some(Utc::now());
                a.triggered_value = Some(last);
            }
        }
    }

    pub fn list_alerts(&self, status: Option<AlertStatus>) -> Vec<Alert> {
        let mut v: Vec<Alert> = self.alerts.lock().unwrap().values().filter(|a| status.is_none_or(|s| a.status == s)).cloned().collect();
        v.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        v
    }

    // ─── forecasting ─────────────────────────────────────────────────────

    /// Forecast `horizon` future points via Holt-style linear trend on closes:
    /// fits level + average slope and projects forward. Returns the projection
    /// and the in-sample drift. Powers the Demand Forecast & Dispatch agents.
    pub fn forecast(&self, instrument_id: &str, horizon: usize) -> Option<serde_json::Value> {
        let closes = self.closes(instrument_id);
        match crate::analytics::forecast(&closes, horizon) {
            Some((drift, last, projection, mae)) => Some(serde_json::json!({
                "instrument_id": instrument_id,
                "method": "linear_drift",
                "last": last,
                "drift_per_period": drift,
                "horizon": projection.len(),
                "forecast": projection,
                "in_sample_mae": mae,
            })),
            None => Some(serde_json::json!({"instrument_id": instrument_id, "note": "insufficient history for forecast"})),
        }
    }

    // ─── official marks (gated write) ──────────────────────────────────────

    /// Publish an official mark for an instrument. Real-world weight: it feeds
    /// downstream valuation/P&L/collateral, so it is the gated external write.
    pub fn publish_mark(&self, instrument_id: &str, price: f64, as_of: NaiveDate, source: &str, published_by: &str) -> std::result::Result<OfficialMark, String> {
        let inst = self.get_instrument(instrument_id).ok_or_else(|| format!("Instrument not found: {instrument_id}"))?;
        if price <= 0.0 && inst.asset_class != AssetClass::Rate {
            return Err(format!("mark price must be positive for {:?}", inst.asset_class));
        }
        let m = OfficialMark { id: self.next("MRK"), instrument_id: instrument_id.to_string(), price, currency: inst.currency.clone(), as_of, source: source.to_string(), published_by: published_by.to_string(), published_at: Utc::now() };
        self.marks.lock().unwrap().push(m.clone());
        self.audit(published_by, "publish_mark", format!("{instrument_id} {price} @ {as_of}"));
        Ok(m)
    }

    /// Latest official mark for an instrument (most recent as_of).
    pub fn latest_mark(&self, instrument_id: &str) -> Option<OfficialMark> {
        self.marks.lock().unwrap().iter().filter(|m| m.instrument_id == instrument_id).max_by(|a, b| a.as_of.cmp(&b.as_of).then(a.published_at.cmp(&b.published_at))).cloned()
    }

    pub fn list_marks(&self, instrument_id: Option<&str>, limit: usize) -> Vec<OfficialMark> {
        let marks = self.marks.lock().unwrap();
        let mut v: Vec<OfficialMark> = marks.iter().filter(|m| instrument_id.is_none_or(|id| m.instrument_id == id)).cloned().collect();
        v.sort_by(|a, b| b.published_at.cmp(&a.published_at));
        v.truncate(limit);
        v
    }

    pub fn audit_log(&self, limit: usize) -> Vec<AuditEntry> {
        let log = self.audit_log.lock().unwrap();
        log.iter().rev().take(limit).cloned().collect()
    }

    // ─── seed ────────────────────────────────────────────────────────────

    fn seed(&self) {
        let today = Utc::now().date_naive();

        // Equity with a price history.
        let aapl = self.create_instrument("AAPL", "Apple Inc.", AssetClass::Equity, "USD", "USD", "system");
        let msft = self.create_instrument("MSFT", "Microsoft Corp.", AssetClass::Equity, "USD", "USD", "system");
        seed_walk(self, &aapl.id, today, 180.0, 1.2, &[0.5, -0.8, 1.1, 0.3, -0.4, 0.9, -0.2, 0.6, 0.1, -0.5, 0.7, 0.2]);
        seed_walk(self, &msft.id, today, 410.0, 1.5, &[0.6, -0.5, 0.9, 0.4, -0.3, 0.7, -0.1, 0.5, 0.2, -0.4, 0.6, 0.3]);
        self.set_quote(&aapl.id, 189.9, 190.1, 190.0, "system").ok();
        self.set_quote(&msft.id, 421.8, 422.2, 422.0, "system").ok();

        // An equal-weighted benchmark.
        self.create_benchmark("Tech-2", vec![aapl.id.clone(), msft.id.clone()], "system");

        // FX pairs for conversion + triangulation.
        let eurusd = self.create_instrument("EURUSD", "Euro / US Dollar", AssetClass::Fx, "USD", "USD", "system");
        let gbpusd = self.create_instrument("GBPUSD", "Sterling / US Dollar", AssetClass::Fx, "USD", "USD", "system");
        self.set_quote(&eurusd.id, 1.0848, 1.0852, 1.0850, "system").ok();
        self.set_quote(&gbpusd.id, 1.2698, 1.2702, 1.2700, "system").ok();

        // A USD yield curve.
        self.create_curve("USD Treasury", CurveKind::Yield, "USD", "%", vec![
            CurvePoint { tenor: 0.25, value: 5.30 },
            CurvePoint { tenor: 1.0, value: 5.00 },
            CurvePoint { tenor: 2.0, value: 4.60 },
            CurvePoint { tenor: 5.0, value: 4.20 },
            CurvePoint { tenor: 10.0, value: 4.10 },
            CurvePoint { tenor: 30.0, value: 4.30 },
        ], today, "system");

        // Energy: power instrument + forward curve + demand series for forecasting.
        let pwr = self.create_instrument("PWR_BASE", "Baseload Power", AssetClass::Energy, "USD", "USD/MWh", "system");
        seed_walk(self, &pwr.id, today, 45.0, 2.0, &[1.0, -1.5, 2.0, 0.5, -0.5, 1.5, 0.8, -1.0, 1.2, 0.3, -0.7, 1.1]);
        self.set_quote(&pwr.id, 51.5, 52.5, 52.0, "system").ok();
        self.create_curve("Power Forward", CurveKind::Forward, "USD", "USD/MWh", vec![
            CurvePoint { tenor: 1.0, value: 52.0 },
            CurvePoint { tenor: 3.0, value: 55.0 },
            CurvePoint { tenor: 6.0, value: 60.0 },
            CurvePoint { tenor: 12.0, value: 58.0 },
        ], today, "system");

        // A demand instrument (MWh) with a rising trend for the Demand Forecast agent.
        let demand = self.create_instrument("GRID_DEMAND", "System Load", AssetClass::Energy, "USD", "MWh", "system");
        let mut d = 12000.0;
        for k in 0..14 {
            let date = today - Duration::days(14 - k);
            d += 80.0 + (k as f64 % 3.0) * 25.0; // upward drift
            self.add_bar(&demand.id, date, d, d + 50.0, d - 50.0, d, 0.0, "system").ok();
        }
    }
}

// ─── seed helper ───────────────────────────────────────────────────────────

fn seed_walk(s: &MarketDataStore, id: &str, today: NaiveDate, start: f64, scale: f64, steps: &[f64]) {
    let mut price = start;
    let n = steps.len();
    for (k, step) in steps.iter().enumerate() {
        let date = today - Duration::days((n - k) as i64);
        let open = price;
        price += step * scale;
        let close = price;
        let high = open.max(close) + scale * 0.3;
        let low = open.min(close) - scale * 0.3;
        s.add_bar(id, date, open, high, low, close, 1_000_000.0, "system").ok();
    }
}

// ─── math helpers (shared with the live backend) ──────────────────────────

use crate::analytics::{round4, round6};

