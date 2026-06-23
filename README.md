# trading-backend

A Rust backend that **learns** trading strategies from your notes and then **trades** them for you, providing insights and analytics.

It ingests trading notes (markdown/text), uses a configurable OpenAI-compatible LLM to extract structured, executable rules, and also lets you hand-author rules. A background engine evaluates those rules against live market data (forex + derivative indices), emits signals, and executes trades with a toggleable autonomy level: **paper**, **signals-only**, or **live**.

## Stack
- **Rust** (edition 2021), **Axum** (HTTP), **Tokio** (async runtime)
- **PostgreSQL** + **SQLx** (runtime-checked queries, auto migrations)
- **reqwest** for broker / LLM HTTP calls
- **rust_decimal** for precise money math
- **tracing** for structured logs

## Architecture

```
src/
  main.rs          - entrypoint: config, pool, migrations, engine loop, HTTP server
  config.rs        - settings from env vars (.env)
  error.rs         - unified AppError -> HTTP responses
  state.rs         - shared AppState (db, llm, ingest, markets)
  domain/          - Strategy, Rule, Note, Signal, Trade, Account, enums
  db/              - SQLx data access (accounts, strategies, notes, trades, analytics)
  llm/             - OpenAI-compatible chat client (JSON-mode extraction)
  ingest/          - note -> LLM -> structured strategy persistence
  engine/          - rule DSL parser/evaluator + strategy evaluator -> signals
  engine_loop.rs   - background tick: evaluate strategies, manage open trades
  market/          - pluggable MarketProvider (REST forex + derivative-index adapters)
  execution/       - mode-aware signal handling + SL/TP trade management
  insights/        - analytics -> human-readable insights
  backtest.rs      - historical backtest harness (replay candles -> equity curve + stats)
  api/             - Axum routes (accounts, strategies, notes, signals, trades, analytics, backtest)
  analytics.rs     - re-exports analytics types
  migrations/      - SQL schema
```

### How "learning" works
1. You upload a **note** (`POST /accounts/:id/notes`) describing a trading approach.
2. Trigger extraction (`POST /notes/:id`): the LLM is prompted to convert the note into a JSON strategy (symbols, SL/TP in pips/points, risk %, and boolean **rules** in a small DSL).
3. The strategy is persisted with `source = llm` and becomes executable like any manual strategy.
4. You can also author/edit strategies by hand (`POST/PUT /accounts/:id/strategies`).

### Rule DSL
Boolean expressions evaluated against computed indicators:

```
rsi(14) < 30
price > ema(50) and macd() > 0
crossup(ema(50), ema(200))
close < sma(200) or pct_change < -2
```

Functions: `rsi(p)`, `ema(p)`, `sma(p)`, `atr(p)`, `macd()`, `price`/`close`, `high`, `low`, `open`, `volume`, `pct_change`, `cross`, `crossup`, `crossdown`.
Operators: `and or not`, `< <= > >= == !=`, `+ - * /`, parentheses.

### Trading modes (per account, toggleable)
- **paper** — opens simulated trades, tracks PnL against live quotes. No money moves.
- **signals** — records signals only; never opens a trade. Good for reviewing.
- **live** — forwards orders to the broker adapter for real execution.

Switch with `POST /accounts/:id/mode { "mode": "live" }`.

## Setup

### 1. Prerequisites
- Rust toolchain (1.75+)
- PostgreSQL 13+

### 2. Database
```bash
psql -c "CREATE USER trading WITH PASSWORD 'trading';"
psql -c "CREATE DATABASE trading OWNER trading;"
```
Migrations run automatically on startup.

### 3. Configure
```bash
cp .env.example .env
# edit .env: set DATABASE_URL, LLM_API_KEY, LLM_BASE_URL (any OpenAI-compatible endpoint),
#           FOREX_PROVIDER_*, DERIV_PROVIDER_* (your broker REST endpoints)
```

### 4. Run
```bash
cargo run --release
```
Server listens on `http://0.0.0.0:8080`.

## Market providers

### Deriv WebSocket adapter (real market data + live execution)
`src/market/deriv.rs` implements `DerivClient`, a real Deriv WebSocket client that:
- Connects to `wss://ws.derivws.com/websockets/v3?app_id=<DERIV_APP_ID>` with automatic reconnect.
- Authorizes with `DERIV_PROVIDER_API_TOKEN` when set (anonymous market-data mode otherwise).
- Multiplexes request/response over the single socket via `req_id` matching (a background reader task routes responses to per-request oneshot channels).
- **Market data**: `candles` via `ticks_history` (style `candles`, configurable granularity), `quote` via `ticks_history` (style `ticks`).
- **Live orders**: implements the `Broker` trait — builds a `proposal` (CALL=buy / PUT=sell, stake-basis, duration-based, optional `limit_order` stop-loss/take-profit), then `buy`s the contract. Used automatically when an account is in `live` mode and a token is configured.
- Symbol mapping: `EUR/USD` -> `frxEURUSD`; synthetic indices (`R_100`, `BOOM300N`, `CRASH300N`, ...) pass through.

The Deriv client is wired as the derivative-indices `MarketProvider` **and** the live `Broker` in `main.rs`. Set `DERIV_PROVIDER_API_TOKEN` to enable live execution; without it, live mode falls back to simulated fills with a warning.

### Generic REST adapter
`RestProvider` (forex) expects:
- `GET {base}/candles?symbol=..&limit=..` -> `{ "data": [{ "ts","o","h","l","c","v" }] }` (or a bare array; aliases `open/high/low/close/volume` accepted)
- `GET {base}/quote?symbol=..` -> `{ "bid", "ask", "ts" }`

Replace either adapter by implementing the `MarketProvider` (data) and/or `Broker` (orders) traits.

## Backtesting
`POST /strategies/:id/backtest` replays historical candles through a strategy's rules:

```bash
curl -X POST localhost:8080/strategies/<id>/backtest -H 'content-type: application/json' \
  -d '{"symbol":"frxEURUSD","initial_balance":10000,"candles":1000}'
```

The harness (`src/backtest.rs`):
- Fetches candles from the strategy's market provider (Deriv or REST).
- Walks bar-by-bar with a 210-bar warmup, computing indicators over the trailing window and evaluating rules at each close.
- Opens one position at a time on a fresh signal; closes on stop-loss / take-profit hit (checked against bar high/low) or at end-of-data.
- Sizing uses `risk_per_trade` against the stop distance (same logic as live).
- Returns: equity curve, per-trade list, win rate, total/avg PnL, max drawdown %, simplified Sharpe ratio, final equity, total return %.

It is pure (no DB writes), so you can iterate on rules safely before enabling a strategy.

## API reference

| Method | Path | Purpose |
|---|---|---|
| GET | `/health` | Liveness |
| POST / GET | `/accounts` | Create / list accounts |
| GET | `/accounts/:id` | Account detail |
| POST | `/accounts/:id/mode` | Set trading mode (`paper`/`signals`/`live`) |
| POST / GET | `/accounts/:id/strategies` | Create (manual) / list strategies |
| GET / PUT / DELETE | `/strategies/:id` | Get / update / delete a strategy |
| POST / GET | `/accounts/:id/notes` | Upload note / list notes |
| GET / POST | `/notes/:id` | Get note / trigger LLM extraction |
| GET | `/accounts/:id/signals` | Recent signals |
| GET | `/accounts/:id/trades` | Trades |
| POST | `/trades/:id/close` | Manually close a trade |
| GET | `/accounts/:id/analytics` | Summary + per-strategy perf |
| GET | `/accounts/:id/insights` | Narrative insights |
| POST | `/strategies/:id/backtest` | Backtest strategy over historical candles |

### Example: learn from a note
```bash
# 1. create account in paper mode
curl -X POST localhost:8080/accounts -H 'content-type: application/json' \
  -d '{"label":"demo","broker":"deriv","account_ref":"acct1","balance":10000}'

# 2. upload a trading note
curl -X POST localhost:8080/accounts/<acc>/notes -H 'content-type: application/json' \
  -d '{"title":"RSI reversal","content":"Buy EUR/USD when RSI(14) falls below 30 and price is above the 50 EMA. Stop 30 pips, target 60 pips."}'

# 3. extract strategy via LLM
curl -X POST localhost:8080/notes/<note_id>
```

### Example: author a strategy manually
```bash
curl -X POST localhost:8080/accounts/<acc>/strategies -H 'content-type: application/json' -d '{
  "name":"EMA trend",
  "asset_class":"forex",
  "symbols":["EUR/USD"],
  "stop_loss":30,
  "take_profit":60,
  "risk_per_trade":0.01,
  "rules":[
    {"name":"above_ema","expr":"price > ema(50)"},
    {"name":"macd_pos","expr":"macd() > 0"}
  ]
}'
```

## Safety notes
- **Live trading is disabled by default** (`DEFAULT_TRADING_MODE=paper`). The engine only executes real orders when an account is explicitly switched to `live`.
- The LLM extraction produces rules that are then run by a deterministic evaluator; always review extracted strategies before enabling live mode.
- This is research/educational software. Trading forex and derivatives carries substantial risk.

## Roadmap / extension points
- Per-strategy exit-rule DSL (currently SL/TP only)
- WebSocket streaming of candles/signals
- Real broker adapters (Deriv API, IG, OANDA) implementing `MarketProvider`
- Backtesting harness over historical candles
- Multi-strategy portfolio risk limits

## Project layout
Built incrementally; all modules compile with `cargo build`. No live database is required to compile (queries use runtime checking).
