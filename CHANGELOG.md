# Changelog

## [1.1.0] - 2026-06-09

### Added — optional live market data
- **Live backend** selected by `MARKET_DATA_BACKEND=live` (default `memory`). In live mode, `get_quote`, `history`, and `analytics` pull **real data from Yahoo Finance**, and `fx_convert` uses **real ECB reference rates** via Frankfurter — no API key required.
- Live mode returns real numbers or an **honest error**; it never falls back to sample data. Pass a symbol (e.g. `AAPL`, `^GSPC`) as `instrument_id`.
- New `backend_info` tool reports the active backend and live-routed sources (26 tools total).
- Shared `analytics` module so live and memory paths compute identical returns/volatility/forecast math.
- `memory` remains the default and the deterministic offline/test backend.

### Notes
- Free public sources are delayed/best-effort and unofficial; point the live client at a licensed vendor feed for production.
- 18 tests (12 integration + 4 manifest + 2 live unit); live network path verified end-to-end over MCP stdio.

## [1.0.0] - 2026-06-09

Initial release — a broad market-data platform for banking & energy agents.

### Added
- **Instruments & quotes** — instruments across asset classes; bid/ask/last quotes resolvable by id or symbol
  (`create_instrument`, `get_instrument`, `list_instruments`, `set_quote`, `get_quote`)
- **History & analytics** — daily OHLC historian; returns, min/max/mean, daily + annualized volatility, moving averages, and return correlation
  (`add_bar`, `history`, `analytics`, `moving_average`, `correlation`)
- **Curves & FX** — yield/forward curves with linear interpolation (flat extrapolation); FX conversion via direct/inverse/USD-triangulation
  (`create_curve`, `list_curves`, `interpolate_curve`, `fx_convert`)
- **Benchmarks & watchlists** — equal-weighted index levels; watchlists with live quotes
  (`create_benchmark`, `benchmark_level`, `create_watchlist`, `watchlist_quotes`)
- **Alerts & forecasting** — price alerts evaluated on each quote; linear-drift forecasting with in-sample error
  (`create_alert`, `list_alerts`, `forecast`)
- **Official marks** — gated publication of official/closing marks with latest/list lookups
  (`publish_mark`, `latest_mark`, `list_marks`, `audit_log`)
- 25 tools total; `publish_mark` (external write feeding valuation/P&L) is the only approval-gated tool; audit trail on material actions.
- 16 tests (12 integration + 4 manifest); verified end-to-end over MCP stdio.
