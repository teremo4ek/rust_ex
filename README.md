# Expense Tracker API

A production-ready REST API for tracking and analyzing personal expenses. Upload CSV files exported from [Monefy](https://www.monefy.app/) or Euroopt receipt PDFs and get aggregated analytics via JSON endpoints.

Built with Rust to practice async programming, web frameworks, and modern Rust patterns.

## Tech Stack

- **Framework:** axum 0.8
- **Async runtime:** tokio
- **Authentication:** JWT (jsonwebtoken) + Argon2 password hashing
- **CSV parsing:** csv + chrono
- **Money:** rust_decimal — exact decimal amounts, no floating-point drift
- **Validation:** validator
- **Error handling:** thiserror + unified `AppError` with `IntoResponse`
- **Logging:** tracing + tracing-subscriber
- **Configuration:** dotenvy
- **Storage:** SQLite via sqlx 0.8 (runtime queries, embedded migrations)

## Endpoints

### Public

| Method | Path                 | Description             |
| ------ | -------------------- | ----------------------- |
| `GET`  | `/health`            | Health check            |
| `POST` | `/api/auth/register` | Register a new user     |
| `POST` | `/api/auth/login`    | Login and get JWT token |

### Protected (require `Authorization: Bearer <token>`)

| Method   | Path                          | Description                                        |
| -------- | ----------------------------- | -------------------------------------------------- |
| `POST`   | `/api/expenses/upload`        | Upload a Monefy CSV file or a Euroopt receipt PDF  |
| `GET`    | `/api/expenses`               | List expenses with filters and pagination          |
| `DELETE` | `/api/expenses/{id}`          | Delete an expense                                  |
| `GET`    | `/api/expenses/{id}/receipt`  | Receipt line items attached to an expense          |
| `GET`    | `/api/receipts/pending`       | Receipts stored but not yet matched to an expense  |

### Analytics (require `Authorization: Bearer <token>`)

| Method | Path                         | Description                                |
| ------ | ---------------------------- | ------------------------------------------ |
| `GET`  | `/api/analytics/summary`     | Total income, expenses, and net            |
| `GET`  | `/api/analytics/by-category` | Breakdown by expense category              |
| `GET`  | `/api/analytics/by-account`  | Breakdown by account (Cash / Payment card) |
| `GET`  | `/api/analytics/timeline`    | Daily or weekly timeline                   |

Query parameters for analytics: `from`, `to` (dates in `YYYY-MM-DD` format), `group_by` (`day` or `week` for timeline).

Query parameters for expenses list: `from`, `to`, `category`, `account`, `page`, `per_page`.

## Money in JSON

Amounts are stored and aggregated with `rust_decimal` (exact decimal arithmetic). All amounts are positive; the `kind` field (`expense` or `income`) tells the direction. Money fields in JSON responses are strings, preserving exactness:

```json
{
  "total_income": "793",
  "total_expense": "559.17",
  "net": "233.83"
}
```

## Quick Start

```sh
# Configure environment (set DATABASE_URL to a path outside the repo)
cp .env.sample .env

# Build and run the web server
cargo run

# Run the interactive CLI (no server required)
cargo run -- cli

# Run tests
cargo test

# Lint
cargo clippy
```

## CSV Format

The API accepts CSV files in the Monefy export format:

```csv
date,account,category,amount,currency,converted amount,currency,description
4/22/2026,Cash,Food,-17.26,BYN,-17.26,BYN,Groceries
4/22/2026,Cash,Deposits,100,BYN,100,BYN,
```

- Date format: `M/D/YYYY`
- Negative `amount` = expense, positive = income (import normalizes rows to a positive `amount` plus a `kind` of `expense` or `income`)
- Categories: any non-empty category from the export is accepted (Monefy defaults and custom ones, e.g. "Eating out")
- Supported accounts: Cash, Payment card

## Receipt PDFs

`upload` also accepts Euroopt receipt PDFs (FastReport exports with a text layer — no OCR). Requires `pdftotext` (poppler) on `PATH`.

Receipts **never create or modify transactions** — the CSV stays the single source of truth for balances and analytics. A receipt *enriches* an existing expense:

1. The parser extracts the date, total, account (card vs cash), the unique fiscal id (`УИ`), and every line item (code, name, unit price, quantity, discount, line total); the items sum is cross-checked against the receipt total.
2. The receipt is matched against the user's expenses by date + amount == total (`kind = expense`, currency `BYN`, no receipt attached yet); the first match in insertion order wins.
3. Exactly one match → the items are stored as a sub-table under that expense (`GET /api/expenses/{id}/receipt` returns them).
4. No match → the receipt is stored as **pending** with a clear warning (`GET /api/receipts/pending`, CLI `receipts`); every later CSV import retries matching and reports `receipts_attached`.
5. Re-uploading a receipt with the same `УИ` is reported as a duplicate; if the stored copy is still pending, the re-upload retries matching it against a free expense.
6. Deleting an expense reverts its receipt to pending (`ON DELETE SET NULL`), so it re-attaches when the CSV row reappears.

PDF upload response example:

```json
{
  "status": "attached",
  "date": "2026-11-02",
  "total": "11.51",
  "currency": "BYN",
  "item_count": 4,
  "expense_id": "0b0e8b4e-…",
  "converted": 0,
  "skipped": 0
}
```

`status` is `attached`, `pending`, or `duplicate`.

## Example Usage

```bash
# Register
curl -X POST http://localhost:3000/api/auth/register \
  -H "Content-Type: application/json" \
  -d '{"email": "user@example.com", "password": "password123"}'

# Login
curl -X POST http://localhost:3000/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email": "user@example.com", "password": "password123"}'

# Upload CSV
curl -X POST http://localhost:3000/api/expenses/upload \
  -H "Authorization: Bearer <token>" \
  -F "file=@monefy.csv"

# Get summary
curl http://localhost:3000/api/analytics/summary \
  -H "Authorization: Bearer <token>"

# Get breakdown by category
curl http://localhost:3000/api/analytics/by-category?from=2026-04-01&to=2026-04-30 \
  -H "Authorization: Bearer <token>"

# Get daily timeline
curl http://localhost:3000/api/analytics/timeline?group_by=day \
  -H "Authorization: Bearer <token>"
```

## CLI Mode

An interactive console session with the same data pipeline, no web server required:

```sh
cargo run -- cli
```

| Command                              | Description                                          |
| ------------------------------------ | ---------------------------------------------------- |
| `upload <path>`                      | Import a Monefy CSV or Euroopt receipt PDF           |
| `stats`                              | Overview: summary + category breakdown               |
| `stats summary / category / account` | Single view                                          |
| `stats timeline [day\|week]`         | Spending over time                                   |
| `receipts`                           | Show pending receipts (waiting for a matching CSV import) |
| `from <date>` / `to <date>`          | Optional date filters (YYYY-MM-DD)                   |
| `help` / `exit`                      | Usage / quit                                         |

Data persists in the SQLite database across runs: the CLI loads `.env` and requires `DATABASE_URL` exactly like the server; no HTTP listener is started.

## Project Structure

```
src/
├── lib.rs                  # library root (used by integration tests)
├── main.rs                 # dispatch: no args → server, "cli" → console mode
├── common/                 # shared between web and CLI
│   ├── config.rs           # environment configuration
│   ├── db.rs               # SQLite pool + migrations
│   ├── error.rs            # unified error type
│   ├── state.rs            # shared application state
│   ├── models/             # User, Expense, DTOs
│   ├── services/           # auth, CSV parsing, analytics (HTTP-free)
│   └── helpers/            # NBRB exchange rate helper
├── web/
│   ├── routes.rs           # route definitions
│   ├── handlers/           # register, login, upload, list, delete, analytics
│   └── middleware/auth.rs  # JWT extractor
└── cli/
    ├── session.rs          # interactive loop (login/register menu, commands)
    ├── commands.rs         # command parsing and execution
    └── views.rs            # text table rendering

tests/
├── common/mod.rs               # test helpers
├── auth_tests.rs               # authentication tests
├── expenses_tests.rs           # expenses + receipt upload tests
├── analytics_tests.rs          # analytics tests
├── cli_tests.rs                # CLI integration tests (spawn the binary)
├── fixtures/monefy.csv         # sample CSV for tests
├── fixtures/receipt_match.csv  # one-row CSV matching check.pdf by date + total
├── fixtures/check.pdf          # Euroopt receipt for tests
└── fixtures/check2.pdf         # Euroopt receipt with per-item discounts
```

## Testing

- **89 unit tests** — CSV and receipt parsing, receipt matching, analytics calculations, JWT generation/validation, password hashing, CLI commands/session
- **35 integration tests** — 25 web flows via `tower::ServiceExt::oneshot`, 10 CLI flows by spawning the real binary (including data persistence across runs)

```sh
cargo test              # run all tests
cargo test -- --test-threads=1  # sequential (useful for shared state)
```

## Key Patterns

- **Custom extractor** — `AuthUser` implements `FromRequestParts<AppState>` to extract user from JWT
- **Unified error handling** — `AppError` enum with `impl IntoResponse` for consistent JSON error responses
- **Connection pool** — `sqlx::SqlitePool` in `AppState`; concurrent SQLite writers serialized by the pool and busy timeout
- **Separation of concerns** — handlers (HTTP layer), services (business logic), models (data types)
- **Exact money** — `rust_decimal::Decimal` end-to-end, serialized as JSON strings
