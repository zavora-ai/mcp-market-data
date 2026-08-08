# Market Data MCP Server

[![Crates.io](https://img.shields.io/crates/v/mcp-market-data.svg)](https://crates.io/crates/mcp-market-data)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![ADK-Rust Enterprise](https://img.shields.io/badge/ADK--Rust-Enterprise-purple.svg)](https://enterprise.adk-rust.com)
[![Registry Ready](https://img.shields.io/badge/ADK_Registry-Ready-green.svg)](https://www.zavora.ai)

A market-data platform for [ADK-Rust Enterprise](https://enterprise.adk-rust.com) banking and energy agents. 26 MCP tools covering instruments across asset classes, real-time quotes and historical bars, analytics (returns, volatility, correlation, moving averages), **yield & forward curves with interpolation**, **FX conversion**, benchmarks/indices, watchlists, price alerts, and **forecasting** — plus a single gated write to publish official marks.

**Real data, optionally.** By default the server runs on a deterministic in-memory backend (great for offline use and tests). Set `MARKET_DATA_BACKEND=live` and quotes, history, analytics, and FX pull **real market data** from free public sources — no API key required. See [Backends](#backends).

## A platform, not a point solution

This is modeled as a general market/reference-data backbone (à la a Bloomberg/Refinitiv-style data service), so banking and energy agents are clients of one shared platform:

| Agent | Domain | Uses |
|-------|--------|------|
| **Treasury Liquidity Analyst** | banking | `interpolate_curve` (yield), `fx_convert`, `analytics`, `publish_mark` |
| **Demand Forecast Agent** | energy-utilities | `history`, `forecast`, `moving_average` |
| **Renewable Dispatch Assistant** | energy-utilities | `interpolate_curve` (forward), `forecast`, `create_alert`, `get_quote` |

## Architecture

<p align="center">
  <img src="https://raw.githubusercontent.com/zavora-ai/mcp-market-data/main/docs/architecture.svg" alt="Market Data MCP Architecture" width="780"/>
</p>

## Capabilities

- **Instruments & quotes** — instruments across equity/bond/fx/commodity/rate/energy/index; latest bid/ask/last quotes (resolvable by id or symbol).
- **Historical bars** — daily OHLC history (the historian).
- **Analytics** — period return, min/max/mean, daily and **annualized volatility** (×√252), simple moving averages, and **return correlation** (Pearson) between instruments.
- **Curves** — yield and forward term structures with **linear interpolation** (flat extrapolation beyond the ends).
- **FX** — convert between currencies using direct, inverse, or **USD-triangulated** rates.
- **Benchmarks** — equal-weighted index levels with member contributions.
- **Watchlists & alerts** — instrument watchlists with live quotes; price alerts (above/below) evaluated on every new quote.
- **Forecasting** — linear-drift projection over a close series with in-sample error, for demand/price forecasting.
- **Official marks** — publish a closing/official mark (the gated write).

## Governance posture

- **One gated write** — `publish_mark` is `external_write` + `requires_approval`: an official mark feeds downstream valuation, P&L, NAV, and collateral, so it carries real-world weight. Every other tool is a read or an internal data ingest.
- **Everything material is audited** — mark publication and data changes append to an audit trail (`audit_log`).
- This is a reference/analytics engine with fictitious seed data; forecasts and analytics are simple, transparent methods (not investment advice).

## Backends

The data source is selected by the `MARKET_DATA_BACKEND` environment variable:

| Value | Behavior |
|-------|----------|
| `memory` (default) | Deterministic seeded data. Offline, fast, used by the test suite. |
| `live` | Real market data from free, no-API-key public sources. |

In **live** mode these tools route to real sources (everything else is unchanged):

| Tool | Live source |
|------|-------------|
| `get_quote` | Yahoo Finance chart API (real-time/delayed price) |
| `history` | Yahoo Finance chart API (daily OHLC) |
| `analytics` | computed from real Yahoo Finance history |
| `fx_convert` | ECB reference rates via [Frankfurter](https://frankfurter.dev) |

In live mode, pass a **symbol** (e.g. `AAPL`, `^GSPC`, `MSFT`) as the `instrument_id`. Tools return real numbers or an **honest error** — they never fall back to sample data. `backend_info` reports the active backend and which sources are in use.

```bash
MARKET_DATA_BACKEND=live mcp-market-data
```

> The free sources are delayed/best-effort and unofficial. For production-grade, low-latency or licensed data, point the live client at a proper vendor feed — the tool contract stays the same.

## Tools (26)

### Instruments & Quotes (5)
`create_instrument` · `get_instrument` · `list_instruments` · `set_quote` · `get_quote`

### History & Analytics (5)
`add_bar` · `history` · `analytics` · `moving_average` · `correlation`

### Curves & FX (4)
`create_curve` · `list_curves` · `interpolate_curve` · `fx_convert`

### Benchmarks & Watchlists (4)
`create_benchmark` · `benchmark_level` · `create_watchlist` · `watchlist_quotes`

### Alerts & Forecasting (3)
`create_alert` · `list_alerts` · `forecast`

### Official Marks, Audit & Ops (5)
`publish_mark` (gated, external) · `latest_mark` · `list_marks` · `audit_log` · `backend_info`

## Example

```jsonc
// Treasury: interpolate the yield curve and convert FX
{"name": "interpolate_curve", "arguments": {"curve_id": "CRV-1010", "tenor": 3.0}}
{"name": "fx_convert", "arguments": {"amount": 10000000, "from": "EUR", "to": "GBP"}}

// Energy: forecast demand, check a forward, set a dispatch alert
{"name": "forecast", "arguments": {"instrument_id": "INS-1018", "horizon": 7}}
{"name": "create_alert", "arguments": {"instrument_id": "INS-1014", "condition": "above", "threshold": 60}}

// Gated: publish an official mark
{"name": "publish_mark", "arguments": {"instrument_id": "INS-1001", "price": 190.25}}
```

## Install & run

```bash
cargo install mcp-market-data
mcp-market-data            # serves MCP over stdio
```

Or build from source:

```bash
git clone https://github.com/zavora-ai/mcp-market-data
cd mcp-market-data && cargo build --release
./target/release/mcp-market-data
```

## Registry manifest

```toml
server_id = "mcp_market_data"
display_name = "Market Data"
version = "1.1.0"
domain = "banking"
risk_level = "medium"
writes_allowed = "gated"
```

The full [`mcp-server.toml`](mcp-server.toml) declares all 25 tools with risk classes and approval gates for registry onboarding.

## License

Apache-2.0

## rmcp and MCP compatibility

This server is built with [`rmcp` 3.1.2](https://github.com/modelcontextprotocol/rust-sdk/releases/tag/rmcp-v3.1.2) and requires Rust 1.88 or newer. The rmcp 3 rollout retains legacy MCP initialization compatibility and targets MCP protocol revisions `2025-11-25` and `2026-07-28`.
