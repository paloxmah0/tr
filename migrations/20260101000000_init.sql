-- Enums
CREATE TYPE trading_mode AS ENUM ('paper', 'signals', 'live');
CREATE TYPE asset_class AS ENUM ('forex', 'derivindex');
CREATE TYPE strategy_source AS ENUM ('manual', 'llm');
CREATE TYPE side AS ENUM ('buy', 'sell');
CREATE TYPE order_type AS ENUM ('market', 'limit', 'stop');
CREATE TYPE trade_status AS ENUM ('open', 'closed', 'rejected', 'cancelled');
CREATE TYPE note_status AS ENUM ('pending', 'extracted', 'failed');

CREATE TABLE accounts (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    label        TEXT NOT NULL,
    broker       TEXT NOT NULL,
    account_ref  TEXT NOT NULL,
    mode         trading_mode NOT NULL DEFAULT 'paper',
    balance      NUMERIC(20,4) NOT NULL DEFAULT 0,
    currency     TEXT NOT NULL DEFAULT 'USD',
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE strategies (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id     UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    name           TEXT NOT NULL,
    description    TEXT,
    asset_class    asset_class NOT NULL,
    symbols        JSONB NOT NULL DEFAULT '[]',
    stop_loss      NUMERIC(20,4),
    take_profit    NUMERIC(20,4),
    risk_per_trade NUMERIC(10,4) NOT NULL DEFAULT 0.01,
    enabled        BOOLEAN NOT NULL DEFAULT true,
    source         strategy_source NOT NULL DEFAULT 'manual',
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_strategies_account ON strategies(account_id);

CREATE TABLE rules (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    strategy_id  UUID NOT NULL REFERENCES strategies(id) ON DELETE CASCADE,
    name         TEXT NOT NULL,
    expr         TEXT NOT NULL,
    weight       NUMERIC(10,4) NOT NULL DEFAULT 1.0,
    enabled      BOOLEAN NOT NULL DEFAULT true
);
CREATE INDEX idx_rules_strategy ON rules(strategy_id);

CREATE TABLE notes (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id   UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    title        TEXT NOT NULL,
    content      TEXT NOT NULL,
    content_type TEXT NOT NULL DEFAULT 'markdown',
    status       note_status NOT NULL DEFAULT 'pending',
    error        TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    processed_at TIMESTAMPTZ
);
CREATE INDEX idx_notes_account ON notes(account_id);

CREATE TABLE signals (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    strategy_id  UUID NOT NULL REFERENCES strategies(id) ON DELETE CASCADE,
    account_id   UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    symbol       TEXT NOT NULL,
    side         side NOT NULL,
    price        NUMERIC(20,6) NOT NULL,
    strength     NUMERIC(10,4) NOT NULL,
    rationale    TEXT NOT NULL,
    mode         trading_mode NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_signals_account ON signals(account_id);
CREATE INDEX idx_signals_strategy ON signals(strategy_id);

CREATE TABLE trades (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id   UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    strategy_id  UUID NOT NULL REFERENCES strategies(id) ON DELETE CASCADE,
    signal_id    UUID REFERENCES signals(id) ON DELETE SET NULL,
    symbol       TEXT NOT NULL,
    side         side NOT NULL,
    order_type   order_type NOT NULL DEFAULT 'market',
    mode         trading_mode NOT NULL,
    size         NUMERIC(20,4) NOT NULL,
    entry_price  NUMERIC(20,6) NOT NULL,
    exit_price   NUMERIC(20,6),
    stop_loss    NUMERIC(20,6),
    take_profit  NUMERIC(20,6),
    pnl          NUMERIC(20,4),
    status       trade_status NOT NULL DEFAULT 'open',
    opened_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    closed_at    TIMESTAMPTZ
);
CREATE INDEX idx_trades_account ON trades(account_id);
CREATE INDEX idx_trades_status ON trades(status);
