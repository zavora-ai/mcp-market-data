//! Optional live market-data backend.
//!
//! Fetches REAL data from free, no-API-key public sources:
//!   - Yahoo Finance chart API (query1.finance.yahoo.com) — real-time/delayed
//!     equity & index quotes and daily OHLC history as JSON. Used for quotes,
//!     history, and analytics.
//!   - Frankfurter (https://frankfurter.dev, ECB reference rates) — daily FX.
//!
//! Selected via `MARKET_DATA_BACKEND=live` (default `memory`). In live mode the
//! tools return real numbers or an HONEST error — they never fall back to the
//! seeded sample data. The in-memory backend remains the default and the
//! offline/test path.
//!
//! NOTE: these free sources are delayed/best-effort and unofficial. For
//! production-grade, low-latency or licensed data, point `Backend` at a proper
//! vendor feed; the routing contract here stays the same.

use crate::analytics;
use crate::types::Bar;
use chrono::NaiveDate;

/// Which data source backs price/FX reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// Seeded in-memory data — default, deterministic, offline.
    Memory,
    /// Live public sources (Yahoo Finance + Frankfurter).
    Live,
}

impl Backend {
    /// Resolve from the `MARKET_DATA_BACKEND` env var (default memory).
    pub fn from_env() -> Backend {
        match std::env::var("MARKET_DATA_BACKEND").unwrap_or_default().to_lowercase().as_str() {
            "live" => Backend::Live,
            _ => Backend::Memory,
        }
    }

    pub fn label(&self) -> &'static str {
        match self { Backend::Memory => "memory", Backend::Live => "live" }
    }
}

/// A live client over the free public sources.
#[derive(Clone)]
pub struct LiveClient {
    http: reqwest::Client,
}

impl Default for LiveClient {
    fn default() -> Self {
        Self::new()
    }
}

impl LiveClient {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .user_agent("mcp-market-data/1.1 (+https://github.com/zavora-ai/mcp-market-data)")
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("reqwest client");
        LiveClient { http }
    }

    /// Latest quote for a symbol via Yahoo Finance chart `meta`. Returns the
    /// regular-market price. The free feed has no firm bid/ask, so callers mirror
    /// `last`.
    pub async fn quote(&self, symbol: &str) -> Result<(f64, NaiveDate), String> {
        let json = self.chart(symbol, "5d").await?;
        let meta = &json["chart"]["result"][0]["meta"];
        let price = meta["regularMarketPrice"].as_f64()
            .ok_or_else(|| format!("yahoo: no price for '{symbol}' (symbol may be unknown)"))?;
        let ts = meta["regularMarketTime"].as_i64().unwrap_or(0);
        let date = chrono::DateTime::from_timestamp(ts, 0).map(|d| d.date_naive()).unwrap_or_else(|| chrono::Utc::now().date_naive());
        Ok((price, date))
    }

    /// Daily OHLC history for a symbol via Yahoo Finance chart JSON.
    pub async fn history(&self, symbol: &str, limit: Option<usize>) -> Result<Vec<Bar>, String> {
        // Pull ~1y then trim; covers analytics' 252-day window.
        let json = self.chart(symbol, "1y").await?;
        let result = &json["chart"]["result"][0];
        let ts = result["timestamp"].as_array().ok_or_else(|| format!("yahoo: no history for '{symbol}'"))?;
        let quote = &result["indicators"]["quote"][0];
        let (opens, highs, lows, closes, vols) = (&quote["open"], &quote["high"], &quote["low"], &quote["close"], &quote["volume"]);
        let mut bars = Vec::new();
        for (i, t) in ts.iter().enumerate() {
            let Some(secs) = t.as_i64() else { continue };
            let date = match chrono::DateTime::from_timestamp(secs, 0) { Some(d) => d.date_naive(), None => continue };
            // Skip rows with null OHLC (Yahoo emits gaps as null).
            let (Some(open), Some(high), Some(low), Some(close)) = (
                opens[i].as_f64(), highs[i].as_f64(), lows[i].as_f64(), closes[i].as_f64(),
            ) else { continue };
            let volume = vols[i].as_f64().unwrap_or(0.0);
            bars.push(Bar { instrument_id: symbol.to_string(), date, open, high, low, close, volume });
        }
        if bars.is_empty() { return Err(format!("yahoo: no parseable history rows for '{symbol}'")); }
        bars.sort_by(|a, b| a.date.cmp(&b.date));
        if let Some(n) = limit {
            if n < bars.len() { bars = bars[bars.len() - n..].to_vec(); }
        }
        Ok(bars)
    }

    /// Analytics computed from REAL Yahoo history (same math as the memory path).
    pub async fn analytics(&self, symbol: &str) -> Result<serde_json::Value, String> {
        let bars = self.history(symbol, Some(252)).await?;
        let closes: Vec<f64> = bars.iter().map(|b| b.close).collect();
        let mut v = analytics::summarize("symbol", symbol, symbol, &closes);
        if let Some(obj) = v.as_object_mut() {
            obj.insert("source".into(), serde_json::json!("yahoo-finance"));
        }
        Ok(v)
    }

    /// Fetch a Yahoo Finance chart for a symbol over a range (parsed JSON).
    async fn chart(&self, symbol: &str, range: &str) -> Result<serde_json::Value, String> {
        let s = map_symbol(symbol);
        let url = format!("https://query1.finance.yahoo.com/v8/finance/chart/{s}?range={range}&interval=1d");
        let body = self.get_text(&url).await?;
        let json: serde_json::Value = serde_json::from_str(&body).map_err(|e| format!("yahoo: bad json: {e}"))?;
        if json["chart"]["result"][0].is_null() {
            let err = json["chart"]["error"]["description"].as_str().unwrap_or("unknown symbol or no data");
            return Err(format!("yahoo: {err} for '{symbol}'"));
        }
        Ok(json)
    }

    /// Real FX conversion via Frankfurter (ECB reference rates).
    pub async fn fx_convert(&self, amount: f64, from: &str, to: &str) -> Result<serde_json::Value, String> {
        let from = from.to_uppercase();
        let to = to.to_uppercase();
        if from == to {
            return Ok(serde_json::json!({"amount": amount, "from": from, "to": to, "rate": 1.0, "result": analytics::round4(amount), "source": "identity"}));
        }
        let url = format!("https://api.frankfurter.dev/v1/latest?base={from}&symbols={to}");
        let body = self.get_text(&url).await?;
        let json: serde_json::Value = serde_json::from_str(&body).map_err(|e| format!("frankfurter: bad json: {e}"))?;
        let rate = json["rates"][&to].as_f64()
            .ok_or_else(|| format!("frankfurter: no rate {from}->{to} (currencies must be ISO codes it supports)"))?;
        let date = json["date"].as_str().unwrap_or("").to_string();
        Ok(serde_json::json!({
            "amount": amount, "from": from, "to": to,
            "rate": analytics::round6(rate), "result": analytics::round4(amount * rate),
            "as_of": date, "source": "frankfurter-ecb",
        }))
    }

    async fn get_text(&self, url: &str) -> Result<String, String> {
        let resp = self.http.get(url).send().await.map_err(|e| format!("request failed: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("upstream {} for {url}", resp.status()));
        }
        resp.text().await.map_err(|e| format!("read body failed: {e}"))
    }
}

/// Map a symbol to Yahoo Finance's convention. Yahoo uses plain uppercase
/// tickers (AAPL), `^` for indices (^GSPC), and suffixes for non-US venues
/// (e.g. `VOD.L`). Anything already containing `.`/`^`/`=` passes through.
fn map_symbol(symbol: &str) -> String {
    let s = symbol.trim();
    if s.contains('.') || s.contains('^') || s.contains('=') {
        return s.to_uppercase();
    }
    s.to_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbol_mapping() {
        assert_eq!(map_symbol("aapl"), "AAPL");
        assert_eq!(map_symbol("AAPL"), "AAPL");
        assert_eq!(map_symbol("^gspc"), "^GSPC");
        assert_eq!(map_symbol("vod.l"), "VOD.L");
    }

    #[test]
    fn backend_from_env_defaults_memory() {
        // Unset or unknown -> memory.
        unsafe { std::env::remove_var("MARKET_DATA_BACKEND"); }
        assert_eq!(Backend::from_env(), Backend::Memory);
    }
}
