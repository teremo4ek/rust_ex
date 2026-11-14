# Code Review Findings — `feature/27-web-landing`

Source: `/code-review` over `git diff main...HEAD`, 2026-11-14.
Status per finding: **fixed** (on this branch) or **backlog**.

## 1. `update_expense` breaks CSV content dedup — backlog

`src/common/services/expense_service.rs:481`

Editing any dedup key field (date / account / category / amount / kind / currency) makes the
original CSV row re-importable: `import_csv` builds its content key from current column values,
so after a PUT edit the key matches nothing and re-uploading the same file inserts the original
row again — the transaction is counted twice.

Scenario: upload a CSV with a 17.26 BYN row → edit that expense to 20 via
`PUT /api/expenses/{id}` → re-upload the same CSV backup → both the edited 20 and the
resurrected 17.26 land in analytics.

Proposal:
- Add a `dedup_key TEXT` column to `expenses`, set once at insert time from the CSV row:
  `"{date}|{account}|{category}|{amount.normalize()}|{kind}|{currency}"`
  (`Decimal::normalize()` keeps `17.260` and `17.26` equal).
- `update_expense` never touches `dedup_key` — the frozen origin identity survives edits.
- `import_csv` dedups against stored keys with
  `SELECT dedup_key FROM expenses WHERE user_id = ? AND dedup_key IS NOT NULL`
  and drops the date-range `BETWEEN` — an edited date would fall outside the CSV range and
  re-import anyway. The stored-value parsing (amount/kind) in the import loop disappears.

## 2. `summary` silently became a USD endpoint that drops unconverted expenses — backlog

`src/common/services/analytics_service.rs:29`, `src/web/handlers/analytics.rs:16`

`GET /api/analytics/summary` now returns `summary_usd`, which `filter_map`s over
`amount_usd`: expenses with a NULL USD amount silently fall out of the totals. EUR (and any
currency other than BYN/USD) never gets `amount_usd`, so those expenses are permanently
excluded; a transient NBRB outage shrinks BYN totals the same way. It is also a silent
contract change versus `main` (mixed original currencies → USD) with no README update, and the
handler gained a `convert_to_usd` call that puts external NBRB HTTPS requests and DB writes
inside a read endpoint.

Proposal:
- Restore `GET /api/analytics/summary` to original-currency totals (`analytics_service::summary`).
- Add `GET /api/analytics/summary_usd` for the web UI: same filters, USD totals, plus an
  `unconverted: usize` count of expenses excluded because `amount_usd IS NULL`, so the drop is
  visible instead of silent (`SummaryUsdResponse` DTO).
- Keep `convert_to_usd` out of read handlers — upload and update paths already convert.
- README documents both endpoints.

## 3. `update_expense` stores currency without case normalization — fixed

`src/common/services/expense_service.rs:481`

A free-text lowercase currency (`byn`) is stored verbatim; `convert_to_usd` matches
`currency = 'BYN'` exactly, so `amount_usd` stays NULL forever and the expense silently drops
out of the USD summary (compounds finding 2).

Fix: `update_expense` and `parse_record` normalize with `trim().to_uppercase()`
(empty-after-trim currency → `BadRequest`); `usd_conversion` matches with `UPPER(currency)`
so already-stored lowercase rows convert too.

## 4. NBRB calls and DB writes inside read/update paths — backlog

`src/web/handlers/analytics.rs:16`, `src/common/services/expense_service.rs:513`

`convert_to_usd` issues serial external HTTPS requests (10s timeout each, one per uncached
date) plus DB writes. It currently runs inside the summary read handler (every UI action:
page change, filter, month shift) and inside `update_expense` for **all** pending rows of the
user on every PUT. With NBRB unreachable, each UI action stalls up to 10s × dates.

Proposal: the read-path half is resolved by finding 2 (no conversion in handlers). For
`update_expense`, convert only the single edited row instead of the whole pending set.

## 5. Description search runs two full-table fetches per request — backlog

`src/common/services/expense_service.rs:415`

`count_expenses` and `list_expenses` each call `fetch_all_expenses` and re-filter in Rust, so
`GET /api/expenses?description=x` loads and lowercases every matching row twice; the summary's
`get_user_expenses` does a third identical fetch in the same UI refresh.

Proposal: one shared fetch returning `(rows, total)`; or a SQL `LIKE` prefilter with the Rust
pass kept only for Unicode case-folding.

## 6. Receipt item discount renders a double minus — backlog

`static/app.js:306`

The parser normalizes discounts to negative Decimals, and the details-modal template prefixes
another U+2212 sign: a `-0.45` discount renders as `−-0.45` (e.g. the Соседи fixture).

Proposal: drop the hardcoded `−` from the template (or render `abs`).

## 7. Stale `fetchReceipt` response can overwrite the modal — backlog

`static/app.js:292`

No stale-response guard: open expense A (slow `/receipt` response), close, quickly open
expense B — B renders first, then A's late response overwrites `#receipt-block` with A's
receipt while B's fields stay above it. `openDetails`/`resetModal` never cancel the in-flight
fetch.

Proposal: per-request token in `openDetails`; a late response from a superseded request is
discarded.

## 8. `logout()` keeps the previous session's search text — backlog

`static/app.js:62`

`logout` clears only token/email; `enterApp` resets from/to but not `#search`. On a shared
machine the next user's list, totals and summary load silently filtered by the previous
user's search.

Proposal: clear `#search` (and `state.page`) on logout/enterApp.

## 9. `summary_usd` copy-pastes `summary`'s aggregation — backlog

`src/common/services/analytics_service.rs:29`

Two nearly identical three-field folds over the same slice; a future change to net computation
or a new `kind` must be made twice.

Proposal: resolved alongside finding 2 — `summary_usd` becomes a single pass that also counts
unconverted rows; `summary` keeps its fold.
