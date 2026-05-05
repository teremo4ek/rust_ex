# Expense Tracker API

A production-ready REST API for tracking and analyzing personal expenses. Upload CSV files exported from [Monefy](https://www.monefy.app/) and get aggregated analytics via JSON endpoints.

Built with Rust to practice async programming, web frameworks, and modern Rust patterns.

## Tech Stack

- **Framework:** axum 0.8
- **Async runtime:** tokio
- **Authentication:** JWT (jsonwebtoken) + Argon2 password hashing
- **CSV parsing:** csv + chrono
- **Validation:** validator
- **Error handling:** thiserror + unified `AppError` with `IntoResponse`
- **Logging:** tracing + tracing-subscriber
- **Configuration:** dotenvy
- **State:** in-memory via `Arc<tokio::sync::RwLock<...>>`

## Endpoints

### Public

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/health` | Health check |
| `POST` | `/api/auth/register` | Register a new user |
| `POST` | `/api/auth/login` | Login and get JWT token |

### Protected (require `Authorization: Bearer <token>`)

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/expenses/upload` | Upload a Monefy CSV file |
| `GET` | `/api/expenses` | List expenses with filters and pagination |
| `DELETE` | `/api/expenses/{id}` | Delete an expense |

### Analytics (require `Authorization: Bearer <token>`)

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/analytics/summary` | Total income, expenses, and net |
| `GET` | `/api/analytics/by-category` | Breakdown by expense category |
| `GET` | `/api/analytics/by-account` | Breakdown by account (Cash / Payment card) |
| `GET` | `/api/analytics/timeline` | Daily or weekly timeline |

Query parameters for analytics: `from`, `to` (dates in `YYYY-MM-DD` format), `group_by` (`day` or `week` for timeline).

Query parameters for expenses list: `from`, `to`, `category`, `account`, `page`, `per_page`.

## Quick Start

```sh
# Set up environment
cp .env.example .env  # or create .env with JWT_SECRET and PORT

# Build and run
cargo run

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

## Project Structure

```
src/
├── lib.rs                  # library root (used by integration tests)
├── main.rs                 # server startup
├── config.rs               # environment configuration
├── error.rs                # unified error type
├── state.rs                # shared application state
├── middleware/
│   └── auth.rs             # JWT extractor
├── routes/
│   └── mod.rs              # route definitions
├── handlers/
│   ├── auth.rs             # register, login
│   ├── expenses.rs         # upload, list, delete
│   └── analytics.rs        # summary, by-category, by-account, timeline
├── services/
│   ├── auth_service.rs     # auth logic
│   ├── expense_service.rs  # CSV parsing, CRUD
│   └── analytics_service.rs # aggregation logic
└── models/
    ├── user.rs             # User struct
    ├── expense.rs          # Expense, Account, Category
    └── dto.rs              # request/response types with validation

tests/
├── common/mod.rs           # test helpers
├── auth_tests.rs           # authentication tests
├── expenses_tests.rs       # expenses tests
├── analytics_tests.rs      # analytics tests
└── fixtures/monefy.csv     # sample CSV for tests
```

## Testing

- **11 unit tests** — CSV parsing, analytics calculations, JWT generation/validation, password hashing
- **18 integration tests** — full request/response flows via `tower::ServiceExt::oneshot`

```sh
cargo test              # run all tests
cargo test -- --test-threads=1  # sequential (useful for shared state)
```

## Key Patterns

- **Custom extractor** — `AuthUser` implements `FromRequestParts<AppState>` to extract user from JWT
- **Unified error handling** — `AppError` enum with `impl IntoResponse` for consistent JSON error responses
- **Concurrency** — `Arc<RwLock<HashMap<Uuid, User>>>` and `Arc<RwLock<Vec<Expense>>>` for safe async state access
- **Separation of concerns** — handlers (HTTP layer), services (business logic), models (data types)
