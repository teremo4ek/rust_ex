use chrono::{Datelike, Duration, NaiveDate};
use std::collections::HashMap;

use crate::models::dto::{
    AccountBreakdown, CategoryBreakdown, SummaryResponse, TimelinePoint,
};
use crate::models::Expense;

pub fn summary(expenses: &[Expense]) -> SummaryResponse {
    let total_income: f64 = expenses.iter().filter(|e| e.is_income()).map(|e| e.amount).sum();
    let total_expense: f64 = expenses.iter().filter(|e| e.is_expense()).map(|e| e.amount).sum();

    SummaryResponse {
        total_income,
        total_expense,
        net: total_income + total_expense,
    }
}

pub fn by_category(expenses: &[Expense]) -> Vec<CategoryBreakdown> {
    let total: f64 = expenses
        .iter()
        .filter(|e| e.is_expense())
        .map(|e| e.amount.abs())
        .sum();

    let mut by_cat: HashMap<String, f64> = HashMap::new();
    for e in expenses.iter().filter(|e| e.is_expense()) {
        *by_cat
            .entry(e.category.to_string())
            .or_default() += e.amount.abs();
    }

    let mut result: Vec<CategoryBreakdown> = by_cat
        .into_iter()
        .map(|(category, amount)| {
            let percentage = if total > 0.0 {
                (amount / total) * 100.0
            } else {
                0.0
            };
            CategoryBreakdown {
                category,
                amount,
                percentage,
            }
        })
        .collect();

    result.sort_by(|a, b| b.amount.partial_cmp(&a.amount).unwrap_or(std::cmp::Ordering::Equal));
    result
}

pub fn by_account(expenses: &[Expense]) -> Vec<AccountBreakdown> {
    let mut by_acc: HashMap<String, AccountBreakdown> = HashMap::new();

    for e in expenses {
        let entry = by_acc
            .entry(e.account.to_string())
            .or_insert(AccountBreakdown {
                account: e.account.to_string(),
                income: 0.0,
                expense: 0.0,
            });

        if e.is_income() {
            entry.income += e.amount;
        } else {
            entry.expense += e.amount.abs();
        }
    }

    let mut result: Vec<AccountBreakdown> = by_acc.into_values().collect();
    result.sort_by(|a, b| b.income.partial_cmp(&a.income).unwrap_or(std::cmp::Ordering::Equal));
    result
}

pub fn timeline(expenses: &[Expense], group_by: &str) -> Vec<TimelinePoint> {
    let mut points: HashMap<String, (f64, f64)> = HashMap::new();

    for e in expenses {
        let period = match group_by {
            "week" => week_key(e.date),
            _ => e.date.to_string(),
        };

        let entry = points.entry(period).or_default();
        if e.is_income() {
            entry.0 += e.amount;
        } else {
            entry.1 += e.amount.abs();
        }
    }

    let mut result: Vec<TimelinePoint> = points
        .into_iter()
        .map(|(period, (income, expense))| TimelinePoint {
            period,
            income,
            expense,
        })
        .collect();

    result.sort_by(|a, b| a.period.cmp(&b.period));
    result
}

fn week_key(date: NaiveDate) -> String {
    let monday = date - Duration::days(date.weekday().num_days_from_monday() as i64);
    format!("{} - {}", monday, monday + Duration::days(6))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Account, Category};

    fn make_expense(date: &str, category: Category, account: Account, amount: f64) -> Expense {
        Expense {
            id: uuid::Uuid::new_v4(),
            user_id: uuid::Uuid::new_v4(),
            date: NaiveDate::parse_from_str(date, "%m/%d/%Y").unwrap(),
            account,
            category,
            amount,
            currency: "BYN".into(),
            description: String::new(),
        }
    }

    #[test]
    fn test_summary() {
        let expenses = vec![
            make_expense("4/22/2026", Category::Food, Account::Cash, -50.0),
            make_expense("4/22/2026", Category::Deposits, Account::Cash, 100.0),
            make_expense("4/23/2026", Category::Food, Account::PaymentCard, -30.0),
        ];
        let result = summary(&expenses);
        assert!((result.total_income - 100.0).abs() < f64::EPSILON);
        assert!((result.total_expense - (-80.0)).abs() < f64::EPSILON);
        assert!((result.net - 20.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_by_category() {
        let expenses = vec![
            make_expense("4/22/2026", Category::Food, Account::Cash, -60.0),
            make_expense("4/22/2026", Category::Food, Account::Cash, -40.0),
            make_expense("4/22/2026", Category::Transport, Account::Cash, -50.0),
        ];
        let result = by_category(&expenses);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].category, "Food");
        assert!((result[0].percentage - 66.666_666_666_666_66).abs() < 0.01);
        assert_eq!(result[1].category, "Transport");
    }

    #[test]
    fn test_by_account() {
        let expenses = vec![
            make_expense("4/22/2026", Category::Food, Account::Cash, -30.0),
            make_expense("4/22/2026", Category::Deposits, Account::Cash, 50.0),
            make_expense("4/22/2026", Category::Food, Account::PaymentCard, -20.0),
        ];
        let result = by_account(&expenses);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_timeline_day() {
        let expenses = vec![
            make_expense("4/22/2026", Category::Food, Account::Cash, -30.0),
            make_expense("4/22/2026", Category::Deposits, Account::Cash, 50.0),
            make_expense("4/23/2026", Category::Food, Account::Cash, -20.0),
        ];
        let result = timeline(&expenses, "day");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].period, "2026-04-22");
    }
}
