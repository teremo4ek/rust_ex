use crate::common::models::{
    AccountBreakdown, CategoryBreakdown, SummaryResponse, TimelinePoint,
};

pub const HELP: &str = "\
Commands:
  upload <path>                                        import a Monefy CSV file
  stats [from <date>] [to <date>]                      overview: summary + category breakdown
  stats summary [from <date>] [to <date>]              totals
  stats category [from <date>] [to <date>]             spending by category
  stats account [from <date>] [to <date>]              income/expense by account
  stats timeline [day|week] [from <date>] [to <date>]  spending over time
  help                                                 show this help
  exit                                                 quit
Dates are YYYY-MM-DD.";

pub fn render_summary(s: &SummaryResponse) -> String {
    // total_expense is stored negative; displayed as a positive magnitude
    let lines = [
        format!("{:<14}{:>12.2}", "Total income", s.total_income),
        format!("{:<14}{:>12.2}", "Total expense", s.total_expense.abs()),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_summary() {
        let s = SummaryResponse {
            total_income: 1000.0,
            total_expense: -400.0,
            net: 600.0,
        };
        let out = render_summary(&s);
        assert!(out.contains("1000.00"));
        assert!(out.contains("400.00"));
        assert!(out.contains("600.00"));
        assert!(!out.contains("-400.00"));
    }

    #[test]
    fn test_render_categories() {
        let rows = vec![CategoryBreakdown {
            category: "Food".into(),
            amount: 17.26,
            percentage: 45.5,
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
            income: 100.0,
            expense: 50.5,
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
            income: 100.0,
            expense: 50.0,
        }];
        let out = render_timeline(&rows);
        assert!(out.contains("Period"));
        assert!(out.contains("2026-01-05 - 2026-01-11"));
        assert!(out.contains("100.00"));
        assert!(out.contains("50.00"));
    }
}
