CREATE TABLE users (
    id TEXT PRIMARY KEY,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE expenses (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id),
    date TEXT NOT NULL,
    account TEXT NOT NULL,
    category TEXT NOT NULL,
    amount TEXT NOT NULL,
    kind TEXT NOT NULL,
    currency TEXT NOT NULL,
    amount_usd TEXT,
    description TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_expenses_user_date ON expenses(user_id, date);

CREATE TABLE nbrb_rates (
    date TEXT NOT NULL,
    currency TEXT NOT NULL,
    base_currency TEXT NOT NULL,
    rate TEXT NOT NULL,
    PRIMARY KEY (date, currency, base_currency)
);

CREATE TABLE receipts (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id),
    expense_id TEXT UNIQUE REFERENCES expenses(id) ON DELETE SET NULL,
    date TEXT NOT NULL,
    total TEXT NOT NULL,
    currency TEXT NOT NULL DEFAULT 'BYN',
    account TEXT NOT NULL,
    fiscal_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE UNIQUE INDEX idx_receipts_user_fiscal ON receipts(user_id, fiscal_id);

CREATE INDEX idx_receipts_user_pending ON receipts(user_id) WHERE expense_id IS NULL;

CREATE TABLE receipt_items (
    id TEXT PRIMARY KEY,
    receipt_id TEXT NOT NULL REFERENCES receipts(id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    code TEXT NOT NULL DEFAULT '',
    name TEXT NOT NULL,
    unit_price TEXT NOT NULL,
    quantity TEXT NOT NULL,
    discount TEXT NOT NULL DEFAULT '0',
    total TEXT NOT NULL
);

CREATE INDEX idx_receipt_items_receipt ON receipt_items(receipt_id);
