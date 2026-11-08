use std::collections::HashMap;

use axum::extract::Multipart;
use chrono::NaiveDate;
use csv::{ReaderBuilder, StringRecord};
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::common::error::{AppError, AppResult};
use crate::common::models::dto::ExpensesQuery;
use crate::common::models::receipt::ReceiptReport;
use crate::common::models::{Account, Expense, TransactionKind};
use crate::common::state::AppState;

use super::usd_conversion::ConversionReport;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ImportReport {
    pub imported: usize,
    pub duplicates: usize,
    pub receipts_attached: usize,
}

#[derive(Debug, PartialEq)]
pub enum UploadOutcome {
    Csv(ImportReport),
    Receipt(ReceiptReport),
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct DedupKey {
    date: NaiveDate,
    account: String,
    category: String,
    amount: Decimal,
    kind: TransactionKind,
    currency: String,
}

pub async fn upload(
    state: &AppState,
    user_id: Uuid,
    mut multipart: Multipart,
) -> AppResult<(UploadOutcome, ConversionReport)> {
    while let Some(field) = multipart.next_field().await? {
        let name = field.name().unwrap_or_default().to_string();
        if name != "file" {
            continue;
        }

        let data = field.bytes().await?;
        return import_bytes(state, user_id, &data).await;
    }

    Err(AppError::BadRequest("no 'file' field in upload".into()))
}

pub async fn import_bytes(
    state: &AppState,
    user_id: Uuid,
    data: &[u8],
) -> AppResult<(UploadOutcome, ConversionReport)> {
    let outcome = if data.starts_with(b"%PDF-") {
        let report = super::receipt_service::import_receipt(state, user_id, data).await?;
        UploadOutcome::Receipt(report)
    } else if super::receipt_service::looks_like_html(data) {
        let html = std::str::from_utf8(data)
            .map_err(|_| AppError::BadRequest("HTML receipt must be UTF-8".into()))?;
        let report = super::receipt_service::import_html(state, user_id, html).await?;
        UploadOutcome::Receipt(report)
    } else {
        let content = String::from_utf8(data.to_vec())
            .map_err(|_| AppError::BadRequest("CSV file must be UTF-8".into()))?;
        let report = import_csv(state, user_id, &content).await?;
        UploadOutcome::Csv(report)
    };
    let conversion = super::usd_conversion::convert_to_usd(state, user_id).await?;
    Ok((outcome, conversion))
}

pub async fn import_url(
    state: &AppState,
    user_id: Uuid,
    url: &str,
) -> AppResult<(UploadOutcome, ConversionReport)> {
    let data = crate::common::helpers::http::fetch_bytes(url).await?;
    import_bytes(state, user_id, &data).await
}

pub async fn import_csv(
    state: &AppState,
    user_id: Uuid,
    content: &str,
) -> AppResult<ImportReport> {
    let expenses = parse_csv(content, user_id)?;
    if expenses.is_empty() {
        return Ok(ImportReport::default());
    }
    let min_date = expenses.iter().map(|e| e.date).min().unwrap();
    let max_date = expenses.iter().map(|e| e.date).max().unwrap();
    let mut report = ImportReport::default();

    let mut tx = state.pool.begin().await?;
    let existing = sqlx::query_as::<_, (NaiveDate, String, String, String, String, String)>(
        "SELECT date, account, category, amount, kind, currency FROM expenses
         WHERE user_id = ? AND date BETWEEN ? AND ?",
    )
    .bind(user_id.to_string())
    .bind(min_date)
    .bind(max_date)
    .fetch_all(&mut *tx)
    .await?;

    let mut counts: HashMap<DedupKey, usize> = HashMap::new();
    for (date, account, category, amount, kind, currency) in existing {
        let amount: Decimal = amount
            .parse()
            .map_err(|_| AppError::Database("invalid amount stored in expenses".into()))?;
        let kind: TransactionKind = kind
            .parse()
            .map_err(|_| AppError::Database("invalid kind stored in expenses".into()))?;
        *counts
            .entry(DedupKey {
                date,
                account,
                category,
                amount,
                kind,
                currency,
            })
            .or_insert(0) += 1;
    }

    for expense in &expenses {
        let key = DedupKey {
            date: expense.date,
            account: expense.account.to_string(),
            category: expense.category.clone(),
            amount: expense.amount,
            kind: expense.kind,
            currency: expense.currency.clone(),
        };
        if let Some(n) = counts.get_mut(&key)
            && *n > 0
        {
            *n -= 1;
            report.duplicates += 1;
            continue;
        }

        insert_expense(&mut tx, expense).await?;
        report.imported += 1;
    }
    report.receipts_attached =
        super::receipt_service::attach_pending_receipts(&mut tx, user_id).await?;
    tx.commit().await?;

    Ok(report)
}

pub(crate) async fn insert_expense(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    expense: &Expense,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO expenses (id, user_id, date, account, category, amount, kind, currency, description)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(expense.id.to_string())
    .bind(expense.user_id.to_string())
    .bind(expense.date)
    .bind(expense.account.to_string())
    .bind(expense.category.clone())
    .bind(expense.amount.to_string())
    .bind(expense.kind.to_string())
    .bind(expense.currency.clone())
    .bind(expense.description.clone())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub fn parse_csv(content: &str, user_id: Uuid) -> AppResult<Vec<Expense>> {
    let mut reader = ReaderBuilder::new()
        .flexible(true)
        .from_reader(content.as_bytes());

    let headers = reader.headers()?.clone();
    validate_headers(&headers)?;

    let mut expenses = Vec::new();
    for result in reader.records() {
        let record = result?;
        if record.is_empty() || record.get(0).is_none_or(|d| d.is_empty()) {
            continue;
        }
        let expense = parse_record(&record, user_id)?;
        expenses.push(expense);
    }

    Ok(expenses)
}

fn validate_headers(headers: &csv::StringRecord) -> AppResult<()> {
    if headers.len() < 8 {
        return Err(AppError::BadRequest(format!(
            "expected at least 8 columns, got {}",
            headers.len()
        )));
    }
    Ok(())
}

fn parse_record(record: &StringRecord, user_id: Uuid) -> AppResult<Expense> {
    let date_str = record.get(0).unwrap_or_default();
    if date_str.is_empty() {
        return Err(AppError::BadRequest("empty date field".into()));
    }

    let date = NaiveDate::parse_from_str(date_str, "%m/%d/%Y")
        .map_err(|e| format!("invalid date '{}': {}", date_str, e))?;

    let account: Account = record
        .get(1)
        .unwrap_or_default()
        .parse()
        .map_err(|e: String| AppError::BadRequest(e))?;

    let category = record.get(2).unwrap_or_default().trim().to_string();
    if category.is_empty() {
        return Err(AppError::BadRequest("empty category field".into()));
    }

    let parsed: Decimal = record
        .get(3)
        .unwrap_or_default()
        .parse()
        .map_err(|e| format!("invalid amount: {}", e))?;
    let kind = if parsed < Decimal::ZERO {
        TransactionKind::Expense
    } else {
        TransactionKind::Income
    };
    let amount = parsed.abs();

    let currency = record.get(4).unwrap_or_default().trim().to_string();
    let description = record.get(7).unwrap_or_default().trim().to_string();

    Ok(Expense {
        id: Uuid::new_v4(),
        user_id,
        date,
        account,
        category,
        amount,
        kind,
        currency,
        amount_usd: None,
        description,
    })
}

#[derive(Debug, sqlx::FromRow)]
struct ExpenseRow {
    id: String,
    user_id: String,
    date: NaiveDate,
    account: String,
    category: String,
    amount: String,
    kind: String,
    currency: String,
    amount_usd: Option<String>,
    description: String,
}

impl ExpenseRow {
    fn into_expense(self) -> AppResult<Expense> {
        Ok(Expense {
            id: self
                .id
                .parse()
                .map_err(|_| AppError::Database("invalid expense id stored in expenses".into()))?,
            user_id: self
                .user_id
                .parse()
                .map_err(|_| AppError::Database("invalid user id stored in expenses".into()))?,
            date: self.date,
            account: self
                .account
                .parse()
                .map_err(|_| AppError::Database("invalid account stored in expenses".into()))?,
            category: self.category,
            amount: self
                .amount
                .parse()
                .map_err(|_| AppError::Database("invalid amount stored in expenses".into()))?,
            kind: self
                .kind
                .parse()
                .map_err(|_| AppError::Database("invalid kind stored in expenses".into()))?,
            currency: self.currency,
            amount_usd: self
                .amount_usd
                .map(|v| v.parse::<Decimal>())
                .transpose()
                .map_err(|_| AppError::Database("invalid amount_usd stored in expenses".into()))?,
            description: self.description,
        })
    }
}

const EXPENSE_COLUMNS: &str =
    "id, user_id, date, account, category, amount, kind, currency, amount_usd, description";

fn push_filters<'a>(
    qb: &mut sqlx::QueryBuilder<'a, sqlx::Sqlite>,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    category: Option<&'a str>,
    account: Option<&'a str>,
) {
    if let Some(from) = from {
        qb.push(" AND date >= ").push_bind(from);
    }
    if let Some(to) = to {
        qb.push(" AND date <= ").push_bind(to);
    }
    if let Some(category) = category.filter(|c| !c.is_empty()) {
        qb.push(" AND category = ").push_bind(category);
    }
    if let Some(account) = account.filter(|a| !a.is_empty()) {
        qb.push(" AND account = ").push_bind(account);
    }
}

pub async fn list_expenses(
    state: &AppState,
    user_id: Uuid,
    query: &ExpensesQuery,
) -> AppResult<Vec<Expense>> {
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(50).min(100);
    let offset = (page - 1) as i64 * per_page as i64;

    let mut qb = sqlx::QueryBuilder::new(format!(
        "SELECT {EXPENSE_COLUMNS} FROM expenses WHERE user_id = "
    ));
    qb.push_bind(user_id.to_string());
    push_filters(
        &mut qb,
        query.from,
        query.to,
        query.category.as_deref(),
        query.account.as_deref(),
    );
    qb.push(" ORDER BY rowid LIMIT ")
        .push_bind(per_page as i64)
        .push(" OFFSET ")
        .push_bind(offset);

    let rows: Vec<ExpenseRow> = qb.build_query_as().fetch_all(&state.pool).await?;
    rows.into_iter().map(ExpenseRow::into_expense).collect()
}

pub async fn count_expenses(
    state: &AppState,
    user_id: Uuid,
    query: &ExpensesQuery,
) -> AppResult<usize> {
    let mut qb = sqlx::QueryBuilder::new("SELECT COUNT(*) FROM expenses WHERE user_id = ");
    qb.push_bind(user_id.to_string());
    push_filters(
        &mut qb,
        query.from,
        query.to,
        query.category.as_deref(),
        query.account.as_deref(),
    );

    let count: i64 = qb.build_query_scalar().fetch_one(&state.pool).await?;
    Ok(count as usize)
}

pub async fn delete_expense(state: &AppState, user_id: Uuid, expense_id: Uuid) -> AppResult<()> {
    let result = sqlx::query("DELETE FROM expenses WHERE id = ? AND user_id = ?")
        .bind(expense_id.to_string())
        .bind(user_id.to_string())
        .execute(&state.pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("expense not found".into()));
    }
    Ok(())
}

pub async fn get_user_expenses(
    state: &AppState,
    user_id: Uuid,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
) -> AppResult<Vec<Expense>> {
    let mut qb = sqlx::QueryBuilder::new(format!(
        "SELECT {EXPENSE_COLUMNS} FROM expenses WHERE user_id = "
    ));
    qb.push_bind(user_id.to_string());
    push_filters(&mut qb, from, to, None, None);
    qb.push(" ORDER BY rowid");

    let rows: Vec<ExpenseRow> = qb.build_query_as().fetch_all(&state.pool).await?;
    rows.into_iter().map(ExpenseRow::into_expense).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::config::Config;
    use crate::common::models::ReceiptStatus;
    use crate::common::services::auth_service;
    use rust_decimal::dec;

    const VALID_CSV: &str = "\
date,account,category,amount,currency,converted amount,currency,description
4/22/2026,Cash,Food,-17.26,BYN,-17.26,BYN,Сухофрукты
4/22/2026,Cash,Food,-15.31,BYN,-15.31,BYN,Евроопт
4/22/2026,Cash,Deposits,100,BYN,100,BYN,";

    #[test]
    fn test_parse_valid_csv() {
        let user_id = Uuid::new_v4();
        let expenses = parse_csv(VALID_CSV, user_id).unwrap();
        assert_eq!(expenses.len(), 3);
        assert_eq!(expenses[0].category, "Food");
        assert_eq!(expenses[0].amount, dec!(17.26));
        assert_eq!(expenses[0].kind, TransactionKind::Expense);
        assert_eq!(expenses[2].category, "Deposits");
        assert_eq!(expenses[2].amount, dec!(100));
        assert_eq!(expenses[2].kind, TransactionKind::Income);
    }

    #[test]
    fn test_parse_skips_empty_rows() {
        let csv = "\
date,account,category,amount,currency,converted amount,currency,description
4/22/2026,Cash,Food,-10,BYN,-10,BYN,Test
,,,,,,
";
        let expenses = parse_csv(csv, Uuid::new_v4()).unwrap();
        assert_eq!(expenses.len(), 1);
    }

    #[test]
    fn test_parse_invalid_date() {
        let csv = "\
date,account,category,amount,currency,converted amount,currency,description
not-a-date,Cash,Food,-10,BYN,-10,BYN,Test
";
        let result = parse_csv(csv, Uuid::new_v4());
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_accepts_custom_category() {
        let csv = "\
date,account,category,amount,currency,converted amount,currency,description
4/22/2026,Cash,Eating out,-10,BYN,-10,BYN,Test
";
        let expenses = parse_csv(csv, Uuid::new_v4()).unwrap();
        assert_eq!(expenses.len(), 1);
        assert_eq!(expenses[0].category, "Eating out");
    }

    #[test]
    fn test_parse_rejects_empty_category() {
        let csv = "\
date,account,category,amount,currency,converted amount,currency,description
4/22/2026,Cash,,-10,BYN,-10,BYN,Test
";
        let result = parse_csv(csv, Uuid::new_v4());
        assert!(matches!(result, Err(AppError::BadRequest(_))));
    }

    async fn test_state() -> AppState {
        let config = Config {
            jwt_secret: "test_secret".into(),
            jwt_expires_in: 3600,
            port: 0,
            database_url: "sqlite::memory:".into(),
        };
        AppState::from_config(&config).await.unwrap()
    }

    async fn registered_state() -> (AppState, Uuid) {
        let state = test_state().await;
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

    #[tokio::test]
    async fn test_import_csv_inserts_rows() {
        let (state, user_id) = registered_state().await;
        let report = import_csv(&state, user_id, VALID_CSV).await.unwrap();
        assert_eq!(
            report,
            ImportReport {
                imported: 3,
                duplicates: 0,
                receipts_attached: 0
            }
        );
        assert_eq!(
            count_expenses(&state, user_id, &empty_query()).await.unwrap(),
            3
        );
        let expenses = list_expenses(&state, user_id, &empty_query()).await.unwrap();
        assert!(expenses.iter().all(|e| e.amount_usd.is_none()));
    }

    #[tokio::test]
    async fn test_import_csv_same_file_twice_skips_all() {
        let (state, user_id) = registered_state().await;
        import_csv(&state, user_id, VALID_CSV).await.unwrap();
        let report = import_csv(&state, user_id, VALID_CSV).await.unwrap();
        assert_eq!(
            report,
            ImportReport {
                imported: 0,
                duplicates: 3,
                receipts_attached: 0
            }
        );
        assert_eq!(
            count_expenses(&state, user_id, &empty_query()).await.unwrap(),
            3
        );
    }

    #[tokio::test]
    async fn test_import_csv_preserves_existing_description() {
        let (state, user_id) = registered_state().await;
        let seed = "\
date,account,category,amount,currency,converted amount,currency,description
4/22/2026,Cash,Food,-17.26,BYN,-17.26,BYN,Original
";
        import_csv(&state, user_id, seed).await.unwrap();
        let report = import_csv(&state, user_id, VALID_CSV).await.unwrap();
        assert_eq!(
            report,
            ImportReport {
                imported: 2,
                duplicates: 1,
                receipts_attached: 0
            }
        );
        let expenses = list_expenses(&state, user_id, &empty_query()).await.unwrap();
        let seeded = expenses
            .iter()
            .find(|e| e.amount == dec!(17.26))
            .unwrap();
        assert_eq!(seeded.description, "Original");
    }

    #[tokio::test]
    async fn test_import_csv_inserts_surplus_identical_rows() {
        let (state, user_id) = registered_state().await;
        let seed = "\
date,account,category,amount,currency,converted amount,currency,description
4/22/2026,Cash,Food,-2.2,BYN,-2.2,BYN,
";
        import_csv(&state, user_id, seed).await.unwrap();
        let two_identical = "\
date,account,category,amount,currency,converted amount,currency,description
4/22/2026,Cash,Food,-2.2,BYN,-2.2,BYN,
4/22/2026,Cash,Food,-2.2,BYN,-2.2,BYN,
";
        let report = import_csv(&state, user_id, two_identical)
            .await
            .unwrap();
        assert_eq!(
            report,
            ImportReport {
                imported: 1,
                duplicates: 1,
                receipts_attached: 0
            }
        );
        assert_eq!(
            count_expenses(&state, user_id, &empty_query()).await.unwrap(),
            2
        );
    }

    #[tokio::test]
    async fn test_import_csv_amount_scale_variant_is_duplicate() {
        let (state, user_id) = registered_state().await;
        sqlx::query(
            "INSERT INTO expenses (id, user_id, date, account, category, amount, kind, currency, description)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(user_id.to_string())
        .bind(NaiveDate::from_ymd_opt(2026, 4, 22).unwrap())
        .bind("Cash")
        .bind("Food")
        .bind("17.260")
        .bind("expense")
        .bind("BYN")
        .bind("")
        .execute(&state.pool)
        .await
        .unwrap();
        let report = import_csv(&state, user_id, VALID_CSV).await.unwrap();
        assert_eq!(
            report,
            ImportReport {
                imported: 2,
                duplicates: 1,
                receipts_attached: 0
            }
        );
    }

    #[tokio::test]
    async fn test_import_csv_income_does_not_dedup_against_expense() {
        let (state, user_id) = registered_state().await;
        sqlx::query(
            "INSERT INTO expenses (id, user_id, date, account, category, amount, kind, currency, description)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(user_id.to_string())
        .bind(NaiveDate::from_ymd_opt(2026, 4, 22).unwrap())
        .bind("Cash")
        .bind("Food")
        .bind("100")
        .bind("expense")
        .bind("BYN")
        .bind("")
        .execute(&state.pool)
        .await
        .unwrap();
        let csv = "\
date,account,category,amount,currency,converted amount,currency,description
4/22/2026,Cash,Food,100,BYN,100,BYN,Refund
";
        let report = import_csv(&state, user_id, csv).await.unwrap();
        assert_eq!(
            report,
            ImportReport {
                imported: 1,
                duplicates: 0,
                receipts_attached: 0
            }
        );
    }

    #[tokio::test]
    async fn test_import_csv_dedup_scoped_to_user() {
        let (state, user_id) = registered_state().await;
        let (_token, other) = auth_service::register(
            &state,
            "other@example.com".into(),
            "password123".into(),
            &state.jwt_secret,
            state.jwt_expires_in,
        )
        .await
        .unwrap();
        import_csv(&state, user_id, VALID_CSV).await.unwrap();
        let report = import_csv(&state, other.id, VALID_CSV).await.unwrap();
        assert_eq!(
            report,
            ImportReport {
                imported: 3,
                duplicates: 0,
                receipts_attached: 0
            }
        );
    }

    #[tokio::test]
    async fn test_import_csv_dedup_ignores_rows_outside_csv_date_range() {
        let (state, user_id) = registered_state().await;
        sqlx::query(
            "INSERT INTO expenses (id, user_id, date, account, category, amount, kind, currency, description)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(user_id.to_string())
        .bind(NaiveDate::from_ymd_opt(2026, 3, 1).unwrap())
        .bind("Cash")
        .bind("Food")
        .bind("17.26")
        .bind("expense")
        .bind("BYN")
        .bind("")
        .execute(&state.pool)
        .await
        .unwrap();
        let report = import_csv(&state, user_id, VALID_CSV).await.unwrap();
        assert_eq!(
            report,
            ImportReport {
                imported: 3,
                duplicates: 0,
                receipts_attached: 0
            }
        );
        let report = import_csv(&state, user_id, VALID_CSV).await.unwrap();
        assert_eq!(
            report,
            ImportReport {
                imported: 0,
                duplicates: 3,
                receipts_attached: 0
            }
        );
    }

    #[tokio::test]
    async fn test_import_csv_empty_csv_returns_zero_report() {
        let (state, user_id) = registered_state().await;
        let csv = "date,account,category,amount,currency,converted amount,currency,description\n";
        let report = import_csv(&state, user_id, csv).await.unwrap();
        assert_eq!(report, ImportReport::default());
        assert_eq!(
            count_expenses(&state, user_id, &empty_query()).await.unwrap(),
            0
        );
    }

    #[tokio::test]
    async fn test_import_csv_propagates_parse_error() {
        let (state, user_id) = registered_state().await;
        let csv = "\
date,account,category,amount,currency,converted amount,currency,description
not-a-date,Cash,Food,-10,BYN,-10,BYN,Test
";
        let result = import_csv(&state, user_id, csv).await;
        assert!(matches!(result, Err(AppError::BadRequest(_))));
        assert_eq!(
            count_expenses(&state, user_id, &empty_query()).await.unwrap(),
            0
        );
    }

    #[tokio::test]
    async fn test_import_csv_attaches_pending_receipt() {
        let (state, user_id) = registered_state().await;
        let pdf = include_bytes!("../../../tests/fixtures/check.pdf");
        crate::common::services::receipt_service::import_receipt(&state, user_id, pdf)
            .await
            .unwrap();
        let csv = "\
date,account,category,amount,currency,converted amount,currency,description
11/2/2026,Payment card,Food,-11.51,BYN,-11.51,BYN,Euroopt
";
        let report = import_csv(&state, user_id, csv).await.unwrap();
        assert_eq!(
            report,
            ImportReport {
                imported: 1,
                duplicates: 0,
                receipts_attached: 1
            }
        );
        let report = import_csv(&state, user_id, csv).await.unwrap();
        assert_eq!(
            report,
            ImportReport {
                imported: 0,
                duplicates: 1,
                receipts_attached: 0
            }
        );
    }

    const SOSEDI_CSV: &str = "\
date,account,category,amount,currency,converted amount,currency,description
09/15/2026,Payment card,Food,-13.76,BYN,-13.76,BYN,Sosedi
";

    #[tokio::test]
    async fn test_import_bytes_html_receipt_pending() {
        let (state, user_id) = registered_state().await;
        let html = include_bytes!("../../../tests/fixtures/check3.html");
        let (outcome, conversion) = import_bytes(&state, user_id, html).await.unwrap();
        let UploadOutcome::Receipt(report) = outcome else {
            panic!("expected receipt outcome");
        };
        assert_eq!(report.status, ReceiptStatus::Pending);
        assert_eq!(report.item_count, 6);
        assert_eq!(report.total, dec!(13.76));
        assert_eq!(conversion.converted, 0);
    }

    #[tokio::test]
    async fn test_import_bytes_html_attaches_to_csv() {
        let (state, user_id) = registered_state().await;
        crate::common::services::usd_conversion::seed_usd_rate(
            &state,
            chrono::NaiveDate::from_ymd_opt(2026, 9, 15).unwrap(),
            dec!(2.8230),
        )
        .await;
        let (outcome, csv_conversion) = import_bytes(&state, user_id, SOSEDI_CSV.as_bytes())
            .await
            .unwrap();
        assert!(matches!(outcome, UploadOutcome::Csv(_)));
        assert_eq!(csv_conversion.converted, 1);

        let html = include_bytes!("../../../tests/fixtures/check3.html");
        let (outcome, conversion) = import_bytes(&state, user_id, html).await.unwrap();
        let UploadOutcome::Receipt(report) = outcome else {
            panic!("expected receipt outcome");
        };
        assert_eq!(report.status, ReceiptStatus::Attached);
        assert!(report.expense_id.is_some());
        assert_eq!(conversion.converted, 0);
    }

    #[tokio::test]
    async fn test_import_bytes_html_duplicate() {
        let (state, user_id) = registered_state().await;
        let html = include_bytes!("../../../tests/fixtures/check3.html");
        import_bytes(&state, user_id, html).await.unwrap();
        let (outcome, _) = import_bytes(&state, user_id, html).await.unwrap();
        let UploadOutcome::Receipt(report) = outcome else {
            panic!("expected receipt outcome");
        };
        assert_eq!(report.status, ReceiptStatus::Duplicate);
    }
}
