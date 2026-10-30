use axum::extract::Multipart;
use chrono::NaiveDate;
use csv::{ReaderBuilder, StringRecord};
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::common::error::{AppError, AppResult};
use crate::common::models::dto::ExpensesQuery;
use crate::common::models::{Account, Expense};
use crate::common::state::AppState;

pub async fn upload_csv(
    state: &AppState,
    user_id: Uuid,
    mut multipart: Multipart,
) -> AppResult<usize> {
    while let Some(field) = multipart.next_field().await? {
        let name = field.name().unwrap_or_default().to_string();
        if name != "file" {
            continue;
        }

        let data = field.bytes().await?;
        let content = String::from_utf8(data.to_vec())
            .map_err(|_| AppError::BadRequest("CSV file must be UTF-8".into()))?;

        return import_csv(state, user_id, &content).await;
    }

    Err(AppError::BadRequest("no 'file' field in upload".into()))
}

pub async fn import_csv(state: &AppState, user_id: Uuid, content: &str) -> AppResult<usize> {
    let expenses = parse_csv(content, user_id)?;

    let mut tx = state.pool.begin().await?;
    for expense in &expenses {
        sqlx::query(
            "INSERT INTO expenses (id, user_id, date, account, category, amount, currency, description)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(expense.id.to_string())
        .bind(expense.user_id.to_string())
        .bind(expense.date)
        .bind(expense.account.to_string())
        .bind(expense.category.clone())
        .bind(expense.amount.to_string())
        .bind(expense.currency.clone())
        .bind(expense.description.clone())
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;

    Ok(expenses.len())
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

    let amount: Decimal = record
        .get(3)
        .unwrap_or_default()
        .parse()
        .map_err(|e| format!("invalid amount: {}", e))?;

    let currency = record.get(4).unwrap_or_default().to_string();
    let description = record.get(7).unwrap_or_default().trim().to_string();

    Ok(Expense {
        id: Uuid::new_v4(),
        user_id,
        date,
        account,
        category,
        amount,
        currency,
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
    currency: String,
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
            currency: self.currency,
            description: self.description,
        })
    }
}

const EXPENSE_COLUMNS: &str =
    "id, user_id, date, account, category, amount, currency, description";

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
        assert_eq!(expenses[0].amount, dec!(-17.26));
        assert_eq!(expenses[2].category, "Deposits");
        assert!(expenses[2].is_income());
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
        let count = import_csv(&state, user_id, VALID_CSV).await.unwrap();
        assert_eq!(count, 3);
        assert_eq!(
            count_expenses(&state, user_id, &empty_query()).await.unwrap(),
            3
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
}
