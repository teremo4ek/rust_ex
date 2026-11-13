use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;

use chrono::NaiveDate;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::common::error::{AppError, AppResult};
use crate::common::helpers::nbrb;
use crate::common::state::AppState;

const BASE_CURRENCY: &str = "BYN";
const TARGET_CURRENCY: &str = "USD";

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ConversionReport {
    pub converted: usize,
    pub skipped: usize,
}

pub async fn convert_to_usd(state: &AppState, user_id: Uuid) -> AppResult<ConversionReport> {
    convert(state, user_id, |date| nbrb::get_rate(nbrb::Currency::Usd, date)).await
}

async fn convert<F, Fut>(state: &AppState, user_id: Uuid, fetch_rate: F) -> AppResult<ConversionReport>
where
    F: Fn(NaiveDate) -> Fut,
    Fut: Future<Output = AppResult<Decimal>>,
{
    let mut report = ConversionReport::default();

    let copied = sqlx::query(
        "UPDATE expenses SET amount_usd = amount
         WHERE user_id = ? AND currency = ? AND amount_usd IS NULL",
    )
    .bind(user_id.to_string())
    .bind(TARGET_CURRENCY)
    .execute(&state.pool)
    .await?
    .rows_affected();
    report.converted += copied as usize;

    let pending: Vec<PendingRow> = sqlx::query_as(
        "SELECT id, date, amount FROM expenses
         WHERE user_id = ? AND currency = ? AND amount_usd IS NULL
         ORDER BY rowid",
    )
    .bind(user_id.to_string())
    .bind(BASE_CURRENCY)
    .fetch_all(&state.pool)
    .await?;

    // no transaction across the whole step: rate fetching holds HTTP awaits
    // and a SQLite write tx open across them would deadlock concurrent uploads;
    // every statement is individually idempotent, NULL rows retry on next run
    let mut rates: BTreeMap<NaiveDate, Decimal> = BTreeMap::new();
    for date in pending.iter().map(|row| row.date).collect::<BTreeSet<_>>() {
        let rate = match cached_rate(state, date).await? {
            Some(rate) => Some(rate),
            None => match fetch_rate(date).await {
                Ok(rate) if rate > Decimal::ZERO => {
                    store_rate(state, date, rate).await?;
                    Some(rate)
                }
                _ => None,
            },
        };
        if let Some(rate) = rate {
            rates.insert(date, rate);
        }
    }

    for row in pending {
        let Some(rate) = rates.get(&row.date) else {
            report.skipped += 1;
            continue;
        };
        let amount: Decimal = row
            .amount
            .parse()
            .map_err(|_| AppError::Database("invalid amount stored in expenses".into()))?;
        let usd = (amount / rate).round_dp(2);
        sqlx::query("UPDATE expenses SET amount_usd = ? WHERE id = ? AND amount_usd IS NULL")
            .bind(usd.to_string())
            .bind(row.id)
            .execute(&state.pool)
            .await?;
        report.converted += 1;
    }

    Ok(report)
}

#[derive(Debug, sqlx::FromRow)]
struct PendingRow {
    id: String,
    date: NaiveDate,
    amount: String,
}

async fn cached_rate(state: &AppState, date: NaiveDate) -> AppResult<Option<Decimal>> {
    let rate: Option<String> = sqlx::query_scalar(
        "SELECT rate FROM nbrb_rates WHERE date = ? AND currency = ? AND base_currency = ?",
    )
    .bind(date)
    .bind(TARGET_CURRENCY)
    .bind(BASE_CURRENCY)
    .fetch_optional(&state.pool)
    .await?;
    // a corrupt cached value falls through to a refetch, which overwrites it
    Ok(rate.and_then(|r| r.parse().ok()))
}

async fn store_rate(state: &AppState, date: NaiveDate, rate: Decimal) -> AppResult<()> {
    sqlx::query(
        "INSERT OR REPLACE INTO nbrb_rates (date, currency, base_currency, rate)
         VALUES (?, ?, ?, ?)",
    )
    .bind(date)
    .bind(TARGET_CURRENCY)
    .bind(BASE_CURRENCY)
    .bind(rate.to_string())
    .execute(&state.pool)
    .await?;
    Ok(())
}

#[cfg(test)]
pub(crate) async fn seed_usd_rate(state: &AppState, date: NaiveDate, rate: Decimal) {
    store_rate(state, date, rate).await.unwrap();
}

#[cfg(test)]
pub(crate) async fn seed_usd_rates(state: &AppState, rate: Decimal) {
    let expenses = crate::common::services::expense_service::parse_csv(
        include_str!("../../../tests/fixtures/monefy.csv"),
        Uuid::new_v4(),
    )
    .unwrap();
    let dates: BTreeSet<NaiveDate> = expenses.iter().map(|e| e.date).collect();
    for date in dates {
        seed_usd_rate(state, date, rate).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::config::Config;
    use crate::common::models::Expense;
    use crate::common::models::dto::ExpensesQuery;
    use crate::common::services::{auth_service, expense_service};
    use rust_decimal::dec;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const BYN_CSV: &str = "\
date,account,category,amount,currency,converted amount,currency,description
4/22/2026,Cash,Food,-17.26,BYN,-17.26,BYN,Сухофрукты
4/23/2026,Cash,Food,-15.31,BYN,-15.31,BYN,Евроопт
";

    const MORE_BYN_CSV: &str = "\
date,account,category,amount,currency,converted amount,currency,description
4/22/2026,Cash,Transport,-5,BYN,-5,BYN,Автобус
";

    const USD_CSV: &str = "\
date,account,category,amount,currency,converted amount,currency,description
4/22/2026,Cash,Food,-100,USD,-100,USD,Store
";

    const EUR_CSV: &str = "\
date,account,category,amount,currency,converted amount,currency,description
4/22/2026,Cash,Food,-50,EUR,-50,EUR,Store
";

    async fn registered_state() -> (AppState, Uuid) {
        let state = AppState::from_config(&Config {
            jwt_secret: "test_secret".into(),
            jwt_expires_in: 3600,
            port: 0,
            database_url: "sqlite::memory:".into(),
        })
        .await
        .unwrap();
        let (_token, user) = auth_service::register(
            &state,
            "user@example.com".into(),
            "password123".into(),
            &state.jwt_secret,
            state.jwt_expires_in,
        )
        .await
        .unwrap();
        (state, user.id)
    }

    fn empty_query() -> ExpensesQuery {
        ExpensesQuery {
            from: None,
            to: None,
            category: None,
            account: None,
            page: None,
            per_page: None,
        }
    }

    fn ok_fetcher(
        rate: Decimal,
        calls: Arc<AtomicUsize>,
    ) -> impl Fn(NaiveDate) -> std::future::Ready<AppResult<Decimal>> {
        move |_date| {
            calls.fetch_add(1, Ordering::SeqCst);
            std::future::ready(Ok(rate))
        }
    }

    async fn expenses(state: &AppState, user_id: Uuid) -> Vec<Expense> {
        expense_service::list_expenses(state, user_id, &empty_query())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn converts_byn_with_fetched_rate() {
        let (state, user_id) = registered_state().await;
        expense_service::import_csv(&state, user_id, BYN_CSV)
            .await
            .unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let report = convert(&state, user_id, ok_fetcher(dec!(2.8230), calls))
            .await
            .unwrap();
        assert_eq!(report, ConversionReport { converted: 2, skipped: 0 });
        let expenses = expenses(&state, user_id).await;
        assert_eq!(
            expenses[0].amount_usd,
            Some((dec!(15.31) / dec!(2.8230)).round_dp(2))
        );
        assert_eq!(
            expenses[1].amount_usd,
            Some((dec!(17.26) / dec!(2.8230)).round_dp(2))
        );
    }

    #[tokio::test]
    async fn second_run_uses_cache_no_refetch() {
        let (state, user_id) = registered_state().await;
        expense_service::import_csv(&state, user_id, BYN_CSV)
            .await
            .unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let first = convert(&state, user_id, ok_fetcher(dec!(2.8230), calls.clone()))
            .await
            .unwrap();
        assert_eq!(first.converted, 2);

        expense_service::import_csv(&state, user_id, MORE_BYN_CSV)
            .await
            .unwrap();
        let second = convert(&state, user_id, ok_fetcher(dec!(2.8230), calls.clone()))
            .await
            .unwrap();
        assert_eq!(second, ConversionReport { converted: 1, skipped: 0 });
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn fetched_rate_persisted_to_cache() {
        let (state, user_id) = registered_state().await;
        expense_service::import_csv(&state, user_id, MORE_BYN_CSV)
            .await
            .unwrap();
        convert(
            &state,
            user_id,
            ok_fetcher(dec!(2.8230), Arc::new(AtomicUsize::new(0))),
        )
        .await
        .unwrap();
        let rate = cached_rate(&state, NaiveDate::from_ymd_opt(2026, 4, 22).unwrap())
            .await
            .unwrap();
        assert_eq!(rate, Some(dec!(2.8230)));
    }

    #[tokio::test]
    async fn usd_rows_copied_without_fetch() {
        let (state, user_id) = registered_state().await;
        expense_service::import_csv(&state, user_id, USD_CSV)
            .await
            .unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let report = convert(&state, user_id, ok_fetcher(dec!(2.8230), calls.clone()))
            .await
            .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(report, ConversionReport { converted: 1, skipped: 0 });
        let expenses = expenses(&state, user_id).await;
        assert_eq!(expenses[0].amount_usd, Some(dec!(100)));
    }

    #[tokio::test]
    async fn other_currency_left_null() {
        let (state, user_id) = registered_state().await;
        expense_service::import_csv(&state, user_id, EUR_CSV)
            .await
            .unwrap();
        let report = convert(
            &state,
            user_id,
            ok_fetcher(dec!(2.8230), Arc::new(AtomicUsize::new(0))),
        )
        .await
        .unwrap();
        assert_eq!(report, ConversionReport::default());
        let expenses = expenses(&state, user_id).await;
        assert_eq!(expenses[0].amount_usd, None);
    }

    #[tokio::test]
    async fn fetch_failure_skips_date_converts_others() {
        let (state, user_id) = registered_state().await;
        expense_service::import_csv(&state, user_id, BYN_CSV)
            .await
            .unwrap();
        let bad_date = NaiveDate::from_ymd_opt(2026, 4, 22).unwrap();
        let fetcher = move |date: NaiveDate| {
            let result = if date == bad_date {
                Err(AppError::ExternalApi("NBRB unavailable".into()))
            } else {
                Ok(dec!(2.8230))
            };
            std::future::ready(result)
        };
        let report = convert(&state, user_id, fetcher).await.unwrap();
        assert_eq!(report, ConversionReport { converted: 1, skipped: 1 });
        let expenses = expenses(&state, user_id).await;
        assert_eq!(
            expenses[0].amount_usd,
            Some((dec!(15.31) / dec!(2.8230)).round_dp(2))
        );
        assert_eq!(expenses[1].amount_usd, None);
        let cached = cached_rate(&state, bad_date).await.unwrap();
        assert_eq!(cached, None);
    }

    #[tokio::test]
    async fn zero_rate_treated_as_failure() {
        let (state, user_id) = registered_state().await;
        expense_service::import_csv(&state, user_id, BYN_CSV)
            .await
            .unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let report = convert(&state, user_id, ok_fetcher(Decimal::ZERO, calls))
            .await
            .unwrap();
        assert_eq!(report, ConversionReport { converted: 0, skipped: 2 });
        let expenses = expenses(&state, user_id).await;
        assert!(expenses.iter().all(|e| e.amount_usd.is_none()));
    }

    #[tokio::test]
    async fn rerun_after_full_success_is_noop() {
        let (state, user_id) = registered_state().await;
        expense_service::import_csv(&state, user_id, BYN_CSV)
            .await
            .unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        convert(&state, user_id, ok_fetcher(dec!(2.8230), calls.clone()))
            .await
            .unwrap();
        let second = convert(&state, user_id, ok_fetcher(dec!(2.8230), calls.clone()))
            .await
            .unwrap();
        assert_eq!(second, ConversionReport::default());
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }
}
