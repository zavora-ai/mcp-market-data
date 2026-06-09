//! Shared price-series analytics — used by both the in-memory store and the
//! live backend so the numbers are identical regardless of data source.

pub fn round4(x: f64) -> f64 { (x * 10000.0).round() / 10000.0 }
pub fn round6(x: f64) -> f64 { (x * 1_000_000.0).round() / 1_000_000.0 }

pub fn daily_returns(closes: &[f64]) -> Vec<f64> {
    (1..closes.len())
        .filter_map(|i| if closes[i - 1] != 0.0 { Some(closes[i] / closes[i - 1] - 1.0) } else { None })
        .collect()
}

pub fn stddev(xs: &[f64]) -> f64 {
    if xs.len() < 2 { return 0.0; }
    let mean = xs.iter().sum::<f64>() / xs.len() as f64;
    let var = xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (xs.len() - 1) as f64;
    var.sqrt()
}

pub fn pearson(a: &[f64], b: &[f64]) -> Option<f64> {
    let n = a.len().min(b.len());
    if n < 2 { return None; }
    let (a, b) = (&a[..n], &b[..n]);
    let ma = a.iter().sum::<f64>() / n as f64;
    let mb = b.iter().sum::<f64>() / n as f64;
    let mut cov = 0.0; let mut va = 0.0; let mut vb = 0.0;
    for i in 0..n {
        let da = a[i] - ma; let db = b[i] - mb;
        cov += da * db; va += da * da; vb += db * db;
    }
    if va == 0.0 || vb == 0.0 { return None; }
    Some(cov / (va.sqrt() * vb.sqrt()))
}

/// Summary stats over a close series: last, min, max, mean, period return, and
/// daily + annualized volatility. `id_field`/`id_value` and `symbol` let callers
/// label the output for either a stored instrument id or a live symbol.
pub fn summarize(id_field: &str, id_value: &str, symbol: &str, closes: &[f64]) -> serde_json::Value {
    if closes.len() < 2 {
        return serde_json::json!({id_field: id_value, "symbol": symbol, "samples": closes.len(), "note": "insufficient history"});
    }
    let n = closes.len();
    let last = *closes.last().unwrap();
    let first = *closes.first().unwrap();
    let min = closes.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = closes.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let mean = closes.iter().sum::<f64>() / n as f64;
    let period_return = (last / first - 1.0) * 100.0;
    let rets = daily_returns(closes);
    let vol_daily = stddev(&rets);
    let vol_annual = vol_daily * (252.0_f64).sqrt();
    serde_json::json!({
        id_field: id_value,
        "symbol": symbol,
        "samples": n,
        "last": last,
        "min": round4(min),
        "max": round4(max),
        "mean": round4(mean),
        "period_return_pct": round4(period_return),
        "daily_vol_pct": round4(vol_daily * 100.0),
        "annualized_vol_pct": round4(vol_annual * 100.0),
    })
}

/// Simple moving average series over `window`.
pub fn moving_average(closes: &[f64], window: usize) -> Option<Vec<f64>> {
    if window == 0 || closes.len() < window { return None; }
    let mut sma = Vec::new();
    for i in window..=closes.len() {
        let w = &closes[i - window..i];
        sma.push(round4(w.iter().sum::<f64>() / window as f64));
    }
    Some(sma)
}

/// Linear-drift forecast: average period-over-period change projected `horizon`
/// steps from the last close. Returns (drift, last, projection, in_sample_mae).
pub fn forecast(closes: &[f64], horizon: usize) -> Option<(f64, f64, Vec<f64>, f64)> {
    if closes.len() < 3 { return None; }
    let n = closes.len();
    let drift: f64 = (1..n).map(|i| closes[i] - closes[i - 1]).sum::<f64>() / (n - 1) as f64;
    let last = closes[n - 1];
    let h = horizon.clamp(1, 60);
    let projection: Vec<f64> = (1..=h).map(|k| round4(last + drift * k as f64)).collect();
    let mae: f64 = (1..n).map(|i| (closes[i] - (closes[i - 1] + drift)).abs()).sum::<f64>() / (n - 1) as f64;
    Some((round4(drift), last, projection, round4(mae)))
}
