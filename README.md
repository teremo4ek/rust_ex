# Expense Tracker API

A production-ready REST API for tracking and analyzing personal expenses. Upload CSV files exported from [Monefy](https://www.monefy.app/) and get aggregated analytics via JSON endpoints.

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
- **State:** in-memory via `Arc<tokio::sync::RwLock<...>>`

## Endpoints

### Public

| Method | Path                 | Description             |
| ------ | -------------------- | ----------------------- |
| `GET`  | `/health`            | Health check            |
| `POST` | `/api/auth/register` | Register a new user     |
| `POST` | `/api/auth/login`    | Login and get JWT token |

### Protected (require `Authorization: Bearer <token>`)

| Method   | Path                   | Description                               |
| -------- | ---------------------- | ----------------------------------------- |
| `POST`   | `/api/expenses/upload` | Upload a Monefy CSV file                  |
| `GET`    | `/api/expenses`        | List expenses with filters and pagination |
| `DELETE` | `/api/expenses/{id}`   | Delete an expense                         |

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

Amounts are stored and aggregated with `rust_decimal` (exact decimal arithmetic). Money fields in JSON responses are strings, preserving exactness:

```json
{
  "total_income": "793",
  "total_expense": "-559.17",
  "net": "233.83"
}
```

## Quick Start

```sh
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
- Negative `amount` = expense, positive = income
- Supported categories: Food, House, Transport, Deposits, Gifts, Entertainment, Toiletry, Communications, Bills, Salary
- Supported accounts: Cash, Payment card

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

| Command                              | Description                            |
| ------------------------------------ | -------------------------------------- |
| `upload <path>`                      | Import a Monefy CSV file               |
| `stats`                              | Overview: summary + category breakdown |
| `stats summary / category / account` | Single view                            |
| `stats timeline [day\|week]`         | Spending over time                     |
| `from <date>` / `to <date>`          | Optional date filters (YYYY-MM-DD)     |
| `help` / `exit`                      | Usage / quit                           |

State is in-memory and per-run: every session starts empty, no environment variables are required, no HTTP listener is started.

## Project Structure

```
src/
├── lib.rs                  # library root (used by integration tests)
├── main.rs                 # dispatch: no args → server, "cli" → console mode
├── common/                 # shared between web and CLI
│   ├── config.rs           # environment configuration
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
├── common/mod.rs           # test helpers
├── auth_tests.rs           # authentication tests
├── expenses_tests.rs       # expenses tests
├── analytics_tests.rs      # analytics tests
├── cli_tests.rs            # CLI integration tests (spawn the binary)
└── fixtures/monefy.csv     # sample CSV for tests
```

## Testing

- **43 unit tests** — CSV parsing, analytics calculations, JWT generation/validation, password hashing, CLI commands/session
- **24 integration tests** — 19 web flows via `tower::ServiceExt::oneshot`, 5 CLI flows by spawning the real binary

```sh
cargo test              # run all tests
cargo test -- --test-threads=1  # sequential (useful for shared state)
```

## Key Patterns

- **Custom extractor** — `AuthUser` implements `FromRequestParts<AppState>` to extract user from JWT
- **Unified error handling** — `AppError` enum with `impl IntoResponse` for consistent JSON error responses
- **Concurrency** — `Arc<RwLock<HashMap<Uuid, User>>>` and `Arc<RwLock<Vec<Expense>>>` for safe async state access
- **Separation of concerns** — handlers (HTTP layer), services (business logic), models (data types)
- **Exact money** — `rust_decimal::Decimal` end-to-end, serialized as JSON strings
