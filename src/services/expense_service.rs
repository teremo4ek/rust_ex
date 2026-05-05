use axum::extract::Multipart;
use chrono::NaiveDate;
use csv::{ReaderBuilder, StringRecord};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::models::dto::ExpensesQuery;
use crate::models::{Account, Category, Expense};
use crate::state::AppState;

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

        let expenses = parse_csv(&content, user_id)?;
        let count = expenses.len();
        state.expenses.write().await.extend(expenses);
        return Ok(count);
    }

    Err(AppError::BadRequest("no 'file' field in upload".into()))
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
        return Err(AppError::BadRequest(
            format!("expected at least 8 columns, got {}", headers.len()),
        ));
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

    let category: Category = record
        .get(2)
        .unwrap_or_default()
        .parse()
        .map_err(|e: String| AppError::BadRequest(e))?;

    let amount: f64 = record
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

pub async fn list_expenses(
    state: &AppState,
    user_id: Uuid,
    query: &ExpensesQuery,
) -> AppResult<Vec<Expense>> {
    let expenses = state.expenses.read().await;
    let filtered: Vec<Expense> = expenses
        .iter()
        .filter(|e| e.user_id == user_id)
        .filter(|e| match query.from {
            Some(from) => e.date >= from,
            None => true,
        })
        .filter(|e| match query.to {
            Some(to) => e.date <= to,
            None => true,
        })
        .filter(|e| match &query.category {
            Some(cat) if !cat.is_empty() => e.category.to_string() == *cat,
            _ => true,
        })
        .filter(|e| match &query.account {
            Some(acc) if !acc.is_empty() => e.account.to_string() == *acc,
            _ => true,
        })
        .cloned()
        .collect();

    let page = query.page.unwrap_or(1).max(1) as usize;
    let per_page = query.per_page.unwrap_or(50).min(100) as usize;
    let start = (page - 1) * per_page;
    let paginated: Vec<Expense> = filtered.into_iter().skip(start).take(per_page).collect();

    Ok(paginated)
}

pub async fn count_expenses(
    state: &AppState,
    user_id: Uuid,
    query: &ExpensesQuery,
) -> usize {
    let expenses = state.expenses.read().await;
    expenses
        .iter()
        .filter(|e| e.user_id == user_id)
        .filter(|e| match query.from {
            Some(from) => e.date >= from,
            None => true,
        })
        .filter(|e| match query.to {
            Some(to) => e.date <= to,
            None => true,
        })
        .filter(|e| match &query.category {
            Some(cat) if !cat.is_empty() => e.category.to_string() == *cat,
            _ => true,
        })
        .filter(|e| match &query.account {
            Some(acc) if !acc.is_empty() => e.account.to_string() == *acc,
            _ => true,
        })
        .count()
}

pub async fn delete_expense(
    state: &AppState,
    user_id: Uuid,
    expense_id: Uuid,
) -> AppResult<()> {
    let mut expenses = state.expenses.write().await;
    let idx = expenses
        .iter()
        .position(|e| e.id == expense_id && e.user_id == user_id)
        .ok_or(AppError::NotFound("expense not found".into()))?;

    expenses.remove(idx);
    Ok(())
}

pub async fn get_user_expenses(
    state: &AppState,
    user_id: Uuid,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
) -> Vec<Expense> {
    state
        .expenses
        .read()
        .await
        .iter()
        .filter(|e| e.user_id == user_id)
        .filter(|e| match from {
            Some(d) => e.date >= d,
            None => true,
        })
        .filter(|e| match to {
            Some(d) => e.date <= d,
            None => true,
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(expenses[0].category.to_string(), "Food");
        assert!((expenses[0].amount - -17.26).abs() < f64::EPSILON);
        assert_eq!(expenses[2].category.to_string(), "Deposits");
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
    fn test_parse_unknown_category() {
        let csv = "\
date,account,category,amount,currency,converted amount,currency,description
4/22/2026,Cash,UnknownCat,-10,BYN,-10,BYN,Test
";
        let result = parse_csv(csv, Uuid::new_v4());
        assert!(result.is_err());
    }
}
