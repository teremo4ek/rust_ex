# Build & Test

- Build: `cargo build`
- Test all: `cargo test`
- Unit tests only: `cargo test --lib`
- Integration tests only: `cargo test --tests`
- Lint: `cargo clippy`
- Run server: `cargo run`
- Run CLI: `cargo run -- cli`

Always run `cargo clippy` and `cargo test` before reporting work as done.

# Code Style

- No comments unless the WHY is non-obvious (hidden constraint, workaround, surprising behavior)
- No docstrings or multi-line comment blocks
- No backwards-compatibility shims or unused code — delete it
- No `#[allow(dead_code)]` or `_`-prefixed "just in case" variables
- Prefer early return / `?` operator over nesting
- Commit messages in English, imperative mood

# Architecture

- Persistence: SQLite via sqlx 0.8. `src/common/db.rs` builds the pool (`foreign_keys` ON, `create_if_missing`) and runs `sqlx::migrate!()` from `migrations/`. Runtime queries only (`sqlx::query`/`query_as`/`QueryBuilder` + `#[derive(FromRow)]`); no compile-time `query!` macros. `AppState { pool, jwt_secret, jwt_expires_in }`; `from_config` is async and runs migrations
- Layout: `src/common` (shared: models, services, state, config, db, error), `src/web` (HTTP: routes, handlers, middleware), `src/cli` (console session). `src/main.rs` dispatches: no args → server, `cli` → console mode, anything else → usage + exit 2
- Auth: custom `AuthUser` extractor in `src/web/middleware/auth.rs` validates JWT from `Authorization` header
- CSV parsing: Monefy format, 8 columns with duplicate "currency" header — use `csv::StringRecord` manually, not serde deserialization (serde can't handle duplicate column names)
- Error handling: all errors go through `AppError` in `src/common/error.rs` — add new variants there, never return raw library errors from handlers
- Lib+bin layout: `src/lib.rs` exposes modules for integration tests, `src/main.rs` is dispatch + server startup

# Testing

- Unit tests: `#[cfg(test)] mod tests` inside the same file
- Integration tests: `tests/` directory, use `tower::ServiceExt::oneshot` with `axum::body::Body`
- Test helper: `tests/common/mod.rs` — use `create_test_app()` (async; isolated `sqlite::memory:` pool) and `register_and_login()` for setup
- Unit tests seed data via `register` + `import_csv` (AppState has no collections to poke); `tests/cli_tests.rs` injects a unique temp DB file per test via `Command::env("DATABASE_URL", ...)` — dotenvy never overrides preset env vars
- Synthetic CSV fixture: `tests/fixtures/monefy.csv` — use it for upload and analytics tests
- Each test file creates its own app instance (fresh state, no test pollution)

# Debug Mode

- Only compiled in debug builds (`cargo run`), absent in release (`cargo build --release`)
- `src/common/debug_setup.rs` (library module under `#[cfg(debug_assertions)]`, used by both server startup and the CLI) seeds the debug user idempotently (`INSERT ... ON CONFLICT DO NOTHING`), token is fixed
- Credentials: `debug@example.com` / `debug123`
- Token is printed to console on startup and is deterministic (same token every restart)
- Debug JWT secret overrides `.env` `JWT_SECRET` in debug builds
- In debug builds the CLI also seeds this user: auth menu has `3) Continue as debug user (debug@example.com)`, and `1) Login` works with the debug credentials; release builds show only Login/Register

# CLI Mode

- `cargo run -- cli` — interactive session: no HTTP listener, no tracing; loads `.env` and requires `DATABASE_URL` exactly like the server
- Data persists in the DB file across runs; JWT secret stays random per run (token is discarded within the session); debug builds additionally seed the debug user (see Debug Mode)
- Commands: `upload <path>`, `stats [summary|category|account|timeline [day|week]] [from <date>] [to <date>]`, `help`, `exit`/`quit`; EOF (Ctrl-D) exits cleanly
- `src/cli/session.rs` `run()` takes `&mut dyn BufRead` + `&mut dyn Write` so unit tests drive sessions from byte scripts
- `tests/cli_tests.rs` spawns the real binary via `CARGO_BIN_EXE_expense_tracker` with piped stdin

# Gotchas

- Monefy CSV has two columns named "currency" — serde deserialize fails on this, always use `StringRecord` index-based parsing
- CSV reader must use `.flexible(true)` to handle empty trailing rows (Monefy exports these)
- `csv` field indices: 0=date, 1=account, 2=category, 3=amount, 4=currency, 5=converted amount, 6=currency (dup), 7=description
- Account enum serializes as "Cash" and "Payment_card" via `snake_case` — display is "Cash" / "Payment card" (space, not underscore)
- Money is `rust_decimal::Decimal` end-to-end (never `f64`): `Expense.amount`, analytics DTOs, NBRB rate helper. JSON money fields serialize as strings ("793", "-559.17"); `dec!()` needs the `macros` feature
- `Decimal` equality is value-based (`dec!(100) == dec!(100.0)`), scale only affects serialization; CLI renders 2 dp via `{:.2}` (Display supports precision)
- sqlx 0.8 has no rust_decimal support for SQLite: amounts are stored as TEXT — bind `Decimal::to_string()`, parse on read. Never `SUM(amount)` in SQL (SQLite coerces TEXT to REAL → float drift); all Decimal aggregation stays in Rust (`analytics_service`)
- Dates are ISO TEXT in SQLite; bind `NaiveDate` directly (sqlx chrono feature encodes `%F`); `account` column stores display form "Cash"/"Payment card" while API JSON stays `snake_case`
- List order is `ORDER BY rowid` (insertion order — UUID ids would randomize it)
- `sqlite::memory:` is private per connection: test pools need `max_connections(1)` with `idle_timeout`/`max_lifetime` disabled (`src/common/db.rs` handles this)
- `foreign_keys` is per-connection — set via `SqliteConnectOptions` in `src/common/db.rs`, never a one-off PRAGMA
- Migrations are embedded at compile time (`sqlx::migrate!`) — new migration files require a rebuild
