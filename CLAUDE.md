# Build & Test

- Build: `cargo build`
- Test all: `cargo test`; unit only: `cargo test --lib`; integration only: `cargo test --tests`
- Lint: `cargo clippy`
- Run server: `cargo run`; run CLI: `cargo run -- cli`

Always run `cargo clippy` and `cargo test` before reporting work as done.

# Code Style

- No comments unless the WHY is non-obvious (hidden constraint, workaround, surprising behavior)
- No docstrings or multi-line comment blocks
- No backwards-compatibility shims or unused code — delete it
- No `#[allow(dead_code)]` or `_`-prefixed "just in case" variables
- Prefer early return / `?` operator over nesting
- Commit messages in English, imperative mood

# Architecture

- Layout: `src/common` (models, services, state, config, db, error, helpers), `src/web` (routes, handlers, middleware), `src/cli` (console session). `src/lib.rs` exposes modules for tests; `src/main.rs` dispatches: no args → server, `cli` → console mode, anything else → usage + exit 2
- Persistence: SQLite via sqlx 0.8; `src/common/db.rs` builds the pool (`foreign_keys` ON, `create_if_missing`) and runs `sqlx::migrate!()`. Runtime queries only (`query`/`query_as`/`QueryBuilder` + `#[derive(FromRow)]`), no compile-time `query!` macros. `AppState { pool, jwt_secret, jwt_expires_in }`
- Auth: custom `AuthUser` extractor (`src/web/middleware/auth.rs`) validates JWT from the `Authorization` header
- Errors: all go through `AppError` (`src/common/error.rs`) — add new variants there, never return raw library errors from handlers
- Import pipeline: web upload (`POST /api/expenses/upload`, `POST /api/receipts/url`) and CLI `upload` all funnel into `expense_service::import_bytes`, which sniffs content: `%PDF-` → `receipt_service::import_receipt` (external `pdftotext -layout - -`), HTML (`looks_like_html`) → `import_html` (dependency-free `<pre>` extraction), else Monefy CSV; then USD conversion. URL uploads go via `helpers::http::fetch_bytes`: http/https only, 2 MiB cap, max 5 redirects, SSRF guard resolves every hop's host (loopback/private/link-local → BadRequest)
- Receipts NEVER create expenses — CSV is the sole source of transactions. Receipts parse (date/total/account/УИ/items, one shared parser for Euroopt and "Соседи" formats), dedup by `(user_id, fiscal_id)` (`INSERT … ON CONFLICT DO NOTHING` → `status = duplicate`), then attach to a same-date BYN expense without a receipt: exact total wins, else closest within `MATCH_TOLERANCE` (< 1 BYN); no match → pending, auto-attached by the next CSV upload (`attach_pending_receipts` inside the import tx — exact pass for all pending receipts runs before the tolerance pass)
- USD conversion: `usd_conversion::convert_to_usd` fills `expenses.amount_usd` (BYN via `nbrb_rates` cache → NBRB API on miss, USD copies `amount`, other currencies NULL). Best-effort: `amount_usd IS NULL` retries on the next upload

# Testing

- Unit tests: `#[cfg(test)] mod tests` inside the same file; integration tests in `tests/` via `tower::ServiceExt::oneshot` + `axum::body::Body`
- `tests/common/mod.rs`: `create_test_app()` (isolated `sqlite::memory:` pool) and `register_and_login()`; each test file builds its own app
- Seeding: unit tests call services directly (`register` + `import_csv`); CLI tests spawn the real binary (`CARGO_BIN_EXE_expense_tracker`) with a unique temp DB per test (`Command::env("DATABASE_URL", …)` — dotenvy never overrides preset env)
- Conversion tests stay offline: inject a rate fetcher into `usd_conversion::convert`, or seed `nbrb_rates` (`usd_conversion::seed_usd_rate(s)` in-crate; `common::seed_rate(s)` in integration tests)
- Fixtures: `monefy.csv`, `receipt_match.csv` (one row matching check.pdf), `check.pdf`/`check2.pdf` (Euroopt), `check3.html`/`check4.html` (Соседи) — all sanitized. Fixture УИs (`…A1`–`…A4`) differ from embedded test-const УИs so DB tests don't collide on `(user_id, fiscal_id)`. Receipt tests marked ⚙ need local `pdftotext`; URL tests are validation-only, no network

# Debug Mode

- Debug builds only (`#[cfg(debug_assertions)]`, `src/common/debug_setup.rs`), used by server startup and the CLI
- Seeds `debug@example.com` / `debug123` idempotently; deterministic token printed on startup; debug JWT secret overrides `.env` `JWT_SECRET`
- Debug CLI auth menu adds `3) Continue as debug user`; release builds show only Login/Register

# CLI Mode

- `cargo run -- cli`: interactive session, no HTTP listener or tracing; loads `.env`, requires `DATABASE_URL` like the server
- Commands: `upload <path|url>`, `stats [summary|category|account|timeline [day|week]] [from <date>] [to <date>]`, `receipts`, `help`, `exit`/`quit`; EOF exits cleanly
- `src/cli/session.rs` `run()` takes `&mut dyn BufRead` + `&mut dyn Write` so tests drive sessions from byte scripts

# Gotchas

- Monefy CSV: duplicate "currency" headers break serde — parse `csv::StringRecord` by index (0=date, 1=account, 2=category, 3=amount, 4=currency, 5=converted, 6=currency dup, 7=description) with `.flexible(true)` (empty trailing rows)
- Account enum: `snake_case` in API JSON ("Payment_card") vs display form ("Payment card") — the DB column stores the display form
- Money is `rust_decimal::Decimal` end-to-end, never `f64`; JSON money fields are strings; `dec!()` needs the `macros` feature; CLI renders 2 dp via `{:.2}`
- Amounts are positive everywhere; direction lives in `kind` (`'expense'`/`'income'`) — the sign exists only in CSV input. CSV dedup key includes `kind`
- sqlx 0.8 lacks rust_decimal for SQLite: amounts are TEXT (`Decimal::to_string()` on write, parse on read); never `SUM(amount)` in SQL (TEXT→REAL float drift) — Decimal aggregation stays in Rust (`analytics_service`)
- Dates are ISO TEXT; bind `NaiveDate` directly (sqlx chrono encodes `%F`); web lists order by `date DESC, rowid DESC` (newest first), analytics fetch keeps `rowid` order
- `sqlite::memory:` is private per connection: test pools need `max_connections(1)` and no `idle_timeout`/`max_lifetime` (`src/common/db.rs`); `foreign_keys` is per-connection — set in connect options, never a one-off PRAGMA
- Migrations are compile-time embedded — new files need a rebuild
- NBRB has a rate for every calendar date (no walk-back fallback); `Decimal / 0` panics — conversion guards `rate > 0`; a corrupt cached rate refetches and overwrites (`INSERT OR REPLACE`)
- Receipt matching SQL uses `NOT EXISTS (… r.expense_id = expenses.id)`, never `id NOT IN (…)` — NULL pending receipts make `NOT IN` match nothing
- УИ line: `УИ: <hex>` (Euroopt) or `УИ <hex>` (Соседи) — try both `strip_prefix` forms on the trimmed line; missing УИ → BadRequest (can't dedup)
- Receipt date: a date+time pair beats any bare date (promo footers `с dd.mm.yyyy по …` must not hijack it); discount labels `скидка`/`скидка=`/`скидка:` normalize to `-abs`
- Price multipliers: Cyrillic `х`/`Х` or Latin `x`/`X`, Соседи `*` — strip via `strip_prefix`, never byte slicing (`&s[1..]` panics on 2-byte Cyrillic)
- Card-payment detection is case-insensitive `contains("картой")`
- HTML: lowercase tags with `to_ascii_lowercase` (byte offsets preserved; Unicode `to_lowercase` can change length); `<` opens a tag only before an ASCII letter/`/`/`!`; the tag-end scan skips `>` inside quoted attributes; `looks_like_html` runs after the `%PDF-` sniff and needs `<pre` + delimiter byte, so `<premium` in CSV text can't misroute
- Receipt parsing runs `pdftotext` synchronously in the async context (same tradeoff as argon2); broken stdin pipe → BadRequest, spawn failure (poppler missing) → Internal
