# Build & Test

- Build: `cargo build`
- Test all: `cargo test`
- Unit tests only: `cargo test --lib`
- Integration tests only: `cargo test --tests`
- Lint: `cargo clippy`
- Run server: `cargo run`

Always run `cargo clippy` and `cargo test` before reporting work as done.

# Code Style

- No comments unless the WHY is non-obvious (hidden constraint, workaround, surprising behavior)
- No docstrings or multi-line comment blocks
- No backwards-compatibility shims or unused code — delete it
- No `#[allow(dead_code)]` or `_`-prefixed "just in case" variables
- Prefer early return / `?` operator over nesting
- Commit messages in English, imperative mood

# Architecture

- In-memory state only (`Arc<tokio::sync::RwLock<...>>`). Do not add a database.
- Auth: custom `AuthUser` extractor in `src/middleware/auth.rs` validates JWT from `Authorization` header
- CSV parsing: Monefy format, 8 columns with duplicate "currency" header — use `csv::StringRecord` manually, not serde deserialization (serde can't handle duplicate column names)
- Error handling: all errors go through `AppError` in `src/error.rs` — add new variants there, never return raw library errors from handlers
- Lib+bin layout: `src/lib.rs` exposes modules for integration tests, `src/main.rs` is server startup only

# Testing

- Unit tests: `#[cfg(test)] mod tests` inside the same file
- Integration tests: `tests/` directory, use `tower::ServiceExt::oneshot` with `axum::body::Body`
- Test helper: `tests/common/mod.rs` — use `create_test_app()` and `register_and_login()` for setup
- Real CSV fixture: `tests/fixtures/monefy.csv` — use it for upload and analytics tests
- Each test file creates its own app instance (fresh state, no test pollution)

# Debug Mode

- Only compiled in debug builds (`cargo run`), absent in release (`cargo build --release`)
- `src/debug_setup.rs` auto-creates a user on startup with a fixed JWT token
- Credentials: `debug@example.com` / `debug123`
- Token is printed to console on startup and is deterministic (same token every restart)
- Debug JWT secret overrides `.env` `JWT_SECRET` in debug builds

# Gotchas

- Monefy CSV has two columns named "currency" — serde deserialize fails on this, always use `StringRecord` index-based parsing
- CSV reader must use `.flexible(true)` to handle empty trailing rows (Monefy exports these)
- `csv` field indices: 0=date, 1=account, 2=category, 3=amount, 4=currency, 5=converted amount, 6=currency (dup), 7=description
- Account enum serializes as "Cash" and "Payment_card" via `snake_case` — display is "Cash" / "Payment card" (space, not underscore)
