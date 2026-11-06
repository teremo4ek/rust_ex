use crate::common::models::{
    AccountBreakdown, CategoryBreakdown, Receipt, SummaryResponse, TimelinePoint,
};

pub const HELP: &str = "\
Commands:
  upload <path>                                        import a Monefy CSV or Euroopt receipt PDF
  stats [from <date>] [to <date>]                      overview: summary + category breakdown
  stats summary [from <date>] [to <date>]              totals
  stats category [from <date>] [to <date>]             spending by category
  stats account [from <date>] [to <date>]              income/expense by account
  stats timeline [day|week] [from <date>] [to <date>]  spending over time
  receipts                                             show pending receipts (waiting for a matching CSV import)
  help                                                 show this help
  exit                                                 quit
Dates are YYYY-MM-DD.";

pub fn render_summary(s: &SummaryResponse) -> String {
    let lines = [
        format!("{:<14}{:>12.2}", "Total income", s.total_income),
        format!("{:<14}{:>12.2}", "Total expense", s.total_expense),
        format!("{:<14}{:>12.2}", "Net", s.net),
    ];
    lines.join("\n")
}

pub fn render_categories(rows: &[CategoryBreakdown]) -> String {
    let mut lines = vec![format!("{:<18}{:>10}{:>9}", "Category", "Amount", "Share")];
    for row in rows {
        lines.push(format!(
            "{:<18}{:>10.2}{:>8.2}%",
            row.category, row.amount, row.percentage
        ));
    }
    lines.join("\n")
}

pub fn render_accounts(rows: &[AccountBreakdown]) -> String {
    let mut lines = vec![format!("{:<14}{:>10}{:>10}", "Account", "Income", "Expense")];
    for row in rows {
        lines.push(format!(
            "{:<14}{:>10.2}{:>10.2}",
            row.account, row.income, row.expense
        ));
    }
    lines.join("\n")
}

pub fn render_timeline(rows: &[TimelinePoint]) -> String {
    let mut lines = vec![format!("{:<25}{:>10}{:>10}", "Period", "Income", "Expense")];
    for row in rows {
        lines.push(format!(
            "{:<25}{:>10.2}{:>10.2}",
            row.period, row.income, row.expense
        ));
    }
    lines.join("\n")
}

pub fn render_pending_receipts(receipts: &[Receipt]) -> String {
    let mut lines = vec![format!(
        "{:<12}{:>8}  {:>5}  {}",
        "Date", "Total", "Items", "Account"
    )];
    for receipt in receipts {
        lines.push(format!(
            "{:<12}{:>8}  {:>5}  {}",
            receipt.date, receipt.total, receipt.items.len(), receipt.account
        ));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::dec;

    #[test]
    fn test_render_summary() {
        let s = SummaryResponse {
            total_income: dec!(1000.0),
            total_expense: dec!(400.0),
            net: dec!(600.0),
        };
        let out = render_summary(&s);
        assert!(out.contains("1000.00"));
        assert!(out.contains("400.00"));
        assert!(out.contains("600.00"));
    }

    #[test]
    fn test_render_categories() {
        let rows = vec![CategoryBreakdown {
            category: "Food".into(),
            amount: dec!(17.26),
            percentage: dec!(45.5),
        }];
        let out = render_categories(&rows);
        assert!(out.contains("Category"));
        assert!(out.contains("Food"));
        assert!(out.contains("17.26"));
        assert!(out.contains("45.50%"));
    }

    #[test]
    fn test_render_accounts() {
        let rows = vec![AccountBreakdown {
            account: "Payment card".into(),
            income: dec!(100.0),
            expense: dec!(50.5),
        }];
        let out = render_accounts(&rows);
        assert!(out.contains("Account"));
        assert!(out.contains("Payment card"));
        assert!(out.contains("100.00"));
        assert!(out.contains("50.50"));
    }

    #[test]
    fn test_render_timeline() {
        let rows = vec![TimelinePoint {
            period: "2026-01-05 - 2026-01-11".into(),
            income: dec!(100.0),
            expense: dec!(50.0),
        }];
        let out = render_timeline(&rows);
        assert!(out.contains("Period"));
        assert!(out.contains("2026-01-05 - 2026-01-11"));
        assert!(out.contains("100.00"));
        assert!(out.contains("50.00"));
    }

    #[test]
    fn test_render_pending_receipts() {
        let receipts = vec![Receipt {
            id: uuid::Uuid::new_v4(),
            user_id: uuid::Uuid::new_v4(),
            expense_id: None,
            date: chrono::NaiveDate::from_ymd_opt(2026, 11, 2).unwrap(),
            total: dec!(11.51),
            currency: "BYN".into(),
            account: crate::common::models::Account::PaymentCard,
            fiscal_id: "EDDA8397564658AE0718A4AB".into(),
            items: vec![
                crate::common::models::ReceiptItem {
                    receipt_id: uuid::Uuid::new_v4(),
                    position: 0,
                    code: "1000001".into(),
                    name: "Яблоко РБ 1кг".into(),
                    unit_price: dec!(1.16),
                    quantity: dec!(1.072),
                    discount: dec!(0),
                    total: dec!(1.24),
                },
                crate::common::models::ReceiptItem {
                    receipt_id: uuid::Uuid::new_v4(),
                    position: 1,
                    code: "2000002".into(),
                    name: "Риет-салат".into(),
                    unit_price: dec!(2.99),
                    quantity: dec!(1),
                    discount: dec!(0),
                    total: dec!(2.99),
                },
            ],
        }];
        let out = render_pending_receipts(&receipts);
        assert!(out.contains("Date"));
        assert!(out.contains("2026-11-02"));
        assert!(out.contains("11.51"));
        assert!(out.contains("Payment card"));
    }
}
