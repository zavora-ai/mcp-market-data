//! Validate mcp-server.toml parses, passes SDK validation, has the right tool
//! count, and gates the official-mark publication.

use adk_mcp_sdk::manifest::ServerManifest;
use std::path::Path;

fn manifest() -> ServerManifest {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("mcp-server.toml");
    ServerManifest::from_file(&path).expect("manifest should parse")
}

#[test]
fn manifest_parses_and_validates() {
    let m = manifest();
    assert!(m.validate().is_empty(), "validation errors: {:?}", m.validate());
    assert_eq!(m.server_id, "mcp_market_data");
    assert_eq!(m.domain, "banking");
    assert_eq!(m.tools.len(), 25, "expected 25 declared tools");
}

#[test]
fn publish_mark_is_gated_external() {
    use adk_mcp_sdk::risk::RiskClass;
    let m = manifest();
    let t = m.tools.iter().find(|t| t.name == "publish_mark").unwrap();
    assert!(t.requires_approval, "publish_mark must require approval");
    assert_eq!(t.risk_class, RiskClass::ExternalWrite);
}

#[test]
fn analytics_reads_are_read_only() {
    use adk_mcp_sdk::risk::RiskClass;
    let m = manifest();
    for name in ["get_quote", "history", "analytics", "moving_average", "correlation", "interpolate_curve", "fx_convert", "benchmark_level", "watchlist_quotes", "forecast", "latest_mark", "audit_log"] {
        let t = m.tools.iter().find(|t| t.name == name).unwrap();
        assert_eq!(t.risk_class, RiskClass::ReadOnly, "{name} should be read_only");
    }
}

#[test]
fn only_publish_mark_requires_approval() {
    let m = manifest();
    let gated: Vec<&str> = m.tools.iter().filter(|t| t.requires_approval).map(|t| t.name.as_str()).collect();
    assert_eq!(gated, vec!["publish_mark"], "only publish_mark should be gated");
}
