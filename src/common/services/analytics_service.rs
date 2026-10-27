use chrono::{Datelike, Duration, NaiveDate};
use rust_decimal::{dec, Decimal};
use std::collections::HashMap;

use crate::common::models::dto::{
    AccountBreakdown, CategoryBreakdown, SummaryResponse, TimelinePoint,
};
use crate::common::models::Expense;

pub fn summary(expenses: &[Expense]) -> SummaryResponse {
    let total_income: Decimal = expenses.iter().filter(|e| e.is_income()).map(|e| e.amount).sum();
    let total_expense: Decimal = expenses.iter().filter(|e| e.is_expense()).map(|e| e.amount).sum();

    SummaryResponse {
        total_income,
        total_expense,
        net: total_income + total_expense,
    }
}

pub fn by_category(expenses: &[Expense]) -> Vec<CategoryBreakdown> {
    let total: Decimal = expenses
        .iter()
        .filter(|e| e.is_expense())
        .map(|e| e.amount.abs())
        .sum();

    let mut by_cat: HashMap<String, Decimal> = HashMap::new();
    for e in expenses.iter().filter(|e| e.is_expense()) {
        *by_cat
            .entry(e.category.to_string())
            .or_default() += e.amount.abs();
    }

    let mut result: Vec<CategoryBreakdown> = by_cat
        .into_iter()
        .map(|(category, amount)| {
            let percentage = if total > Decimal::ZERO {
                (amount / total * dec!(100)).round_dp(2)
            } else {
                Decimal::ZERO
            };
            CategoryBreakdown {
                category,
                amount,
                percentage,
            }
        })
        .collect();

    result.sort_by_key(|b| std::cmp::Reverse(b.amount));
    result
}

pub fn by_account(expenses: &[Expense]) -> Vec<AccountBreakdown> {
    let mut by_acc: HashMap<String, AccountBreakdown> = HashMap::new();

    for e in expenses {
        let entry = by_acc
            .entry(e.account.to_string())
            .or_insert(AccountBreakdown {
                account: e.account.to_string(),
                income: Decimal::ZERO,
                expense: Decimal::ZERO,
            });

        if e.is_income() {
            entry.income += e.amount;
        } else {
            entry.expense += e.amount.abs();
        }
    }

    let mut result: Vec<AccountBreakdown> = by_acc.into_values().collect();
    result.sort_by_key(|b| std::cmp::Reverse(b.income));
    result
}

pub fn timeline(expenses: &[Expense], group_by: &str) -> Vec<TimelinePoint> {
    let mut points: HashMap<String, (Decimal, Decimal)> = HashMap::new();

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
    use crate::common::models::{Account, Category};

    fn make_expense(date: &str, category: Category, account: Account, amount: Decimal) -> Expense {
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
            make_expense("4/22/2026", Category::Food, Account::Cash, dec!(-50.0)),
            make_expense("4/22/2026", Category::Deposits, Account::Cash, dec!(100.0)),
            make_expense("4/23/2026", Category::Food, Account::PaymentCard, dec!(-30.0)),
        ];
        let result = summary(&expenses);
        assert_eq!(result.total_income, dec!(100));
        assert_eq!(result.total_expense, dec!(-80));
        assert_eq!(result.net, dec!(20));
    }

    #[test]
    fn test_by_category() {
        let expenses = vec![
            make_expense("4/22/2026", Category::Food, Account::Cash, dec!(-60.0)),
            make_expense("4/22/2026", Category::Food, Account::Cash, dec!(-40.0)),
            make_expense("4/22/2026", Category::Transport, Account::Cash, dec!(-50.0)),
        ];
        let result = by_category(&expenses);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].category, "Food");
        assert_eq!(result[0].amount, dec!(100));
        assert_eq!(result[0].percentage, dec!(66.67));
        assert_eq!(result[1].category, "Transport");
        assert_eq!(result[1].percentage, dec!(33.33));
    }

    #[test]
    fn test_by_account() {
        let expenses = vec![
            make_expense("4/22/2026", Category::Food, Account::Cash, dec!(-30.0)),
            make_expense("4/22/2026", Category::Deposits, Account::Cash, dec!(50.0)),
            make_expense("4/22/2026", Category::Food, Account::PaymentCard, dec!(-20.0)),
        ];
        let result = by_account(&expenses);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_timeline_day() {
        let expenses = vec![
            make_expense("4/22/2026", Category::Food, Account::Cash, dec!(-30.0)),
            make_expense("4/22/2026", Category::Deposits, Account::Cash, dec!(50.0)),
            make_expense("4/23/2026", Category::Food, Account::Cash, dec!(-20.0)),
        ];
        let result = timeline(&expenses, "day");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].period, "2026-04-22");
    }
}
