use chrono::{Datelike, Duration, NaiveDate};
use rust_decimal::{Decimal, dec};
use std::collections::HashMap;

use crate::common::models::{Expense, TransactionKind};
use crate::common::models::dto::{
    AccountBreakdown, CategoryBreakdown, SummaryResponse, TimelinePoint,
};

pub fn summary(expenses: &[Expense]) -> SummaryResponse {
    let total_income: Decimal = expenses
        .iter()
        .filter(|e| e.kind == TransactionKind::Income)
        .map(|e| e.amount)
        .sum();
    let total_expense: Decimal = expenses
        .iter()
        .filter(|e| e.kind == TransactionKind::Expense)
        .map(|e| e.amount)
        .sum();

    SummaryResponse {
        total_income,
        total_expense,
        net: total_income - total_expense,
    }
}

pub fn summary_usd(expenses: &[Expense]) -> SummaryResponse {
    let total_income: Decimal = expenses
        .iter()
        .filter(|e| e.kind == TransactionKind::Income)
        .filter_map(|e| e.amount_usd)
        .sum();
    let total_expense: Decimal = expenses
        .iter()
        .filter(|e| e.kind == TransactionKind::Expense)
        .filter_map(|e| e.amount_usd)
        .sum();

    SummaryResponse {
        total_income,
        total_expense,
        net: total_income - total_expense,
    }
}

pub fn by_category(expenses: &[Expense]) -> Vec<CategoryBreakdown> {
    let total: Decimal = expenses
        .iter()
        .filter(|e| e.kind == TransactionKind::Expense)
        .map(|e| e.amount)
        .sum();

    let mut by_cat: HashMap<String, Decimal> = HashMap::new();
    for e in expenses.iter().filter(|e| e.kind == TransactionKind::Expense) {
        *by_cat.entry(e.category.clone()).or_default() += e.amount;
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

        if e.kind == TransactionKind::Income {
            entry.income += e.amount;
        } else {
            entry.expense += e.amount;
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
        if e.kind == TransactionKind::Income {
            entry.0 += e.amount;
        } else {
            entry.1 += e.amount;
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
    use crate::common::models::Account;

    fn make_expense(
        date: &str,
        category: &str,
        account: Account,
        amount: Decimal,
        kind: TransactionKind,
    ) -> Expense {
        Expense {
            id: uuid::Uuid::new_v4(),
            user_id: uuid::Uuid::new_v4(),
            date: NaiveDate::parse_from_str(date, "%m/%d/%Y").unwrap(),
            account,
            category: category.into(),
            amount,
            kind,
            currency: "BYN".into(),
            amount_usd: None,
            description: String::new(),
        }
    }

    #[test]
    fn test_summary() {
        let expenses = vec![
            make_expense("4/22/2026", "Food", Account::Cash, dec!(50.0), TransactionKind::Expense),
            make_expense(
                "4/22/2026",
                "Deposits",
                Account::Cash,
                dec!(100.0),
                TransactionKind::Income,
            ),
            make_expense(
                "4/23/2026",
                "Food",
                Account::PaymentCard,
                dec!(30.0),
                TransactionKind::Expense,
            ),
        ];
        let result = summary(&expenses);
        assert_eq!(result.total_income, dec!(100));
        assert_eq!(result.total_expense, dec!(80));
        assert_eq!(result.net, dec!(20));
    }

    #[test]
    fn test_summary_usd_skips_unconverted() {
        let mut income =
            make_expense("4/22/2026", "Deposits", Account::Cash, dec!(100.0), TransactionKind::Income);
        income.amount_usd = Some(dec!(35.42));
        let mut expense =
            make_expense("4/22/2026", "Food", Account::Cash, dec!(50.0), TransactionKind::Expense);
        expense.amount_usd = Some(dec!(17.71));
        let unconverted =
            make_expense("4/23/2026", "Food", Account::Cash, dec!(30.0), TransactionKind::Expense);

        let result = summary_usd(&[income, expense, unconverted]);
        assert_eq!(result.total_income, dec!(35.42));
        assert_eq!(result.total_expense, dec!(17.71));
        assert_eq!(result.net, dec!(17.71));
    }

    #[test]
    fn test_by_category() {
        let expenses = vec![
            make_expense("4/22/2026", "Food", Account::Cash, dec!(60.0), TransactionKind::Expense),
            make_expense("4/22/2026", "Food", Account::Cash, dec!(40.0), TransactionKind::Expense),
            make_expense(
                "4/22/2026",
                "Transport",
                Account::Cash,
                dec!(50.0),
                TransactionKind::Expense,
            ),
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
            make_expense("4/22/2026", "Food", Account::Cash, dec!(30.0), TransactionKind::Expense),
            make_expense(
                "4/22/2026",
                "Deposits",
                Account::Cash,
                dec!(50.0),
                TransactionKind::Income,
            ),
            make_expense(
                "4/22/2026",
                "Food",
                Account::PaymentCard,
                dec!(20.0),
                TransactionKind::Expense,
            ),
        ];
        let result = by_account(&expenses);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_timeline_day() {
        let expenses = vec![
            make_expense("4/22/2026", "Food", Account::Cash, dec!(30.0), TransactionKind::Expense),
            make_expense(
                "4/22/2026",
                "Deposits",
                Account::Cash,
                dec!(50.0),
                TransactionKind::Income,
            ),
            make_expense("4/23/2026", "Food", Account::Cash, dec!(20.0), TransactionKind::Expense),
        ];
        let result = timeline(&expenses, "day");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].period, "2026-04-22");
    }
}
