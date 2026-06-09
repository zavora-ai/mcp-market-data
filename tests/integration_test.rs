//! Integration tests: analytics, curve interpolation, FX (incl. triangulation),
//! alerts, forecasting, and the gated official-mark publication.

use chrono::{Duration, Utc};
use mcp_market_data::store::MarketDataStore;
use mcp_market_data::types::*;

fn store() -> MarketDataStore {
    MarketDataStore::new()
}

#[test]
fn seed_loads() {
    let s = store();
    assert!(s.list_instruments(None).len() >= 6);
    assert_eq!(s.list_curves(None).len(), 2);
}

#[test]
fn resolve_by_symbol_or_id() {
    let s = store();
    let by_sym = s.resolve("AAPL").unwrap();
    let by_id = s.resolve(&by_sym.id).unwrap();
    assert_eq!(by_sym.id, by_id.id);
    assert!(s.resolve("NOPE").is_none());
}

#[test]
fn analytics_computes_vol_and_return() {
    let s = store();
    let aapl = s.resolve("AAPL").unwrap();
    let a = s.analytics(&aapl.id).unwrap();
    assert!(a["samples"].as_u64().unwrap() >= 12);
    assert!(a["annualized_vol_pct"].as_f64().unwrap() > 0.0);
    assert!(a["last"].as_f64().is_some());
}

#[test]
fn moving_average_window() {
    let s = store();
    let aapl = s.resolve("AAPL").unwrap();
    let ma = s.moving_average(&aapl.id, 3).unwrap();
    assert!(ma["latest"].as_f64().is_some());
    assert!(!ma["series"].as_array().unwrap().is_empty());
}

#[test]
fn correlation_in_range() {
    let s = store();
    let a = s.resolve("AAPL").unwrap();
    let b = s.resolve("MSFT").unwrap();
    let c = s.correlation(&a.id, &b.id).unwrap();
    let corr = c["correlation"].as_f64().unwrap();
    assert!((-1.0..=1.0).contains(&corr));
}

#[test]
fn curve_interpolation() {
    let s = store();
    let curve = s.list_curves(Some(CurveKind::Yield)).into_iter().next().unwrap();
    // between 2y (4.60) and 5y (4.20): at 3y expect linear ~4.4667
    let r = s.interpolate_curve(&curve.id, 3.0).unwrap();
    let v = r["value"].as_f64().unwrap();
    assert!((v - 4.4667).abs() < 0.01, "got {v}");
    // flat extrapolation below first tenor
    let lo = s.interpolate_curve(&curve.id, 0.05).unwrap();
    assert_eq!(lo["value"].as_f64().unwrap(), 5.30);
    // flat extrapolation above last
    let hi = s.interpolate_curve(&curve.id, 50.0).unwrap();
    assert_eq!(hi["value"].as_f64().unwrap(), 4.30);
}

#[test]
fn fx_direct_inverse_and_triangulated() {
    let s = store();
    // direct EURUSD = 1.0850
    let d = s.fx_convert(100.0, "EUR", "USD").unwrap();
    assert!((d["result"].as_f64().unwrap() - 108.50).abs() < 0.01);
    // inverse USD->EUR
    let inv = s.fx_convert(108.50, "USD", "EUR").unwrap();
    assert!((inv["result"].as_f64().unwrap() - 100.0).abs() < 0.05);
    // triangulated EUR->GBP via USD: 1.0850 / 1.2700
    let tri = s.fx_convert(1.0, "EUR", "GBP").unwrap();
    let rate = tri["rate"].as_f64().unwrap();
    assert!((rate - (1.0850 / 1.2700)).abs() < 0.001, "got {rate}");
    // same currency
    assert_eq!(s.fx_convert(50.0, "USD", "USD").unwrap()["result"].as_f64().unwrap(), 50.0);
}

#[test]
fn benchmark_level_is_mean_of_members() {
    let s = store();
    let bmk = s.create_benchmark("Test", vec![s.resolve("AAPL").unwrap().id, s.resolve("MSFT").unwrap().id], "t");
    let lvl = s.benchmark_level(&bmk.id).unwrap();
    // (190.0 + 422.0)/2 = 306.0
    assert!((lvl["level"].as_f64().unwrap() - 306.0).abs() < 0.01);
    assert_eq!(lvl["members_priced"], 2);
}

#[test]
fn alert_triggers_on_quote() {
    let s = store();
    let aapl = s.resolve("AAPL").unwrap();
    let a = s.create_alert(&aapl.id, AlertCondition::Above, 200.0, "trader").unwrap();
    assert_eq!(a.status, AlertStatus::Armed);
    // last 190 -> not triggered
    s.set_quote(&aapl.id, 195.0, 195.2, 195.0, "feed").unwrap();
    assert_eq!(s.list_alerts(Some(AlertStatus::Triggered)).len(), 0);
    // cross above 200
    s.set_quote(&aapl.id, 201.0, 201.2, 201.0, "feed").unwrap();
    let trig = s.list_alerts(Some(AlertStatus::Triggered));
    assert_eq!(trig.len(), 1);
    assert_eq!(trig[0].triggered_value, Some(201.0));
}

#[test]
fn forecast_projects_trend() {
    let s = store();
    // GRID_DEMAND seeded with an upward drift
    let demand = s.resolve("GRID_DEMAND").unwrap();
    let f = s.forecast(&demand.id, 5).unwrap();
    assert_eq!(f["horizon"], 5);
    let proj = f["forecast"].as_array().unwrap();
    assert_eq!(proj.len(), 5);
    // rising drift -> each forecast point above the last actual
    let last = f["last"].as_f64().unwrap();
    assert!(proj[0].as_f64().unwrap() > last);
    assert!(f["drift_per_period"].as_f64().unwrap() > 0.0);
}

#[test]
fn publish_mark_and_latest() {
    let s = store();
    let aapl = s.resolve("AAPL").unwrap();
    let today = Utc::now().date_naive();
    let m1 = s.publish_mark(&aapl.id, 190.5, today - Duration::days(1), "close", "marker").unwrap();
    let m2 = s.publish_mark(&aapl.id, 191.2, today, "close", "marker").unwrap();
    assert_ne!(m1.id, m2.id);
    let latest = s.latest_mark(&aapl.id).unwrap();
    assert_eq!(latest.price, 191.2, "latest mark by as_of");
    // negative price rejected for an equity
    assert!(s.publish_mark(&aapl.id, -5.0, today, "close", "marker").is_err());
}

#[test]
fn mark_publication_is_audited() {
    let s = store();
    let aapl = s.resolve("AAPL").unwrap();
    s.publish_mark(&aapl.id, 190.0, Utc::now().date_naive(), "close", "marker").unwrap();
    let log = s.audit_log(20);
    assert!(log.iter().any(|e| e.action == "publish_mark" && e.actor == "marker"));
}
