use chrono::NaiveDate;
use uuid::Uuid;

use crate::common::error::{AppError, AppResult};
use crate::common::services::{analytics_service, expense_service};
use crate::common::state::AppState;

use super::views;

#[derive(Debug)]
pub enum Command {
    Upload { path: String },
    Stats {
        view: Option<StatsView>,
        from: Option<NaiveDate>,
        to: Option<NaiveDate>,
    },
    Help,
    Exit,
}

#[derive(Debug)]
pub enum StatsView {
    Summary,
    Category,
    Account,
    Timeline { group_by: GroupBy },
}

#[derive(Debug)]
pub enum GroupBy {
    Day,
    Week,
}

#[derive(Debug)]
pub enum Outcome {
    Output(String),
    Exit,
}

pub fn parse_command(line: &str) -> AppResult<Command> {
    let line = line.trim();
    if line.is_empty() {
        return Err(AppError::BadRequest("empty command".into()));
    }

    let (verb, rest) = match line.split_once(char::is_whitespace) {
        Some((verb, rest)) => (verb, rest.trim()),
        None => (line, ""),
    };

    match verb {
        "upload" => {
            if rest.is_empty() {
                return Err(AppError::BadRequest("usage: upload <path>".into()));
            }
            Ok(Command::Upload {
                path: rest.to_string(),
            })
        }
        "stats" => parse_stats(rest),
        "help" => Ok(Command::Help),
        "exit" | "quit" => Ok(Command::Exit),
        other => Err(AppError::BadRequest(format!(
            "unknown command '{other}', type 'help' for usage"
        ))),
    }
}

fn parse_stats(rest: &str) -> AppResult<Command> {
    let mut view: Option<StatsView> = None;
    let mut from: Option<NaiveDate> = None;
    let mut to: Option<NaiveDate> = None;

    let mut tokens = rest.split_whitespace().peekable();
    while let Some(token) = tokens.next() {
        match token {
            "summary" | "category" | "account" => {
                if view.is_some() {
                    return Err(AppError::BadRequest("duplicate stats view".into()));
                }
                view = Some(match token {
                    "summary" => StatsView::Summary,
                    "category" => StatsView::Category,
                    _ => StatsView::Account,
                });
            }
            "timeline" => {
                if view.is_some() {
                    return Err(AppError::BadRequest("duplicate stats view".into()));
                }
                let group_by = match tokens.peek() {
                    Some(&"day") => {
                        tokens.next();
                        GroupBy::Day
                    }
                    Some(&"week") => {
                        tokens.next();
                        GroupBy::Week
                    }
                    _ => GroupBy::Day,
                };
                view = Some(StatsView::Timeline { group_by });
            }
            "day" | "week" => {
                return Err(AppError::BadRequest(format!(
                    "'{token}' is only valid right after 'timeline'"
                )));
            }
            "from" | "to" => {
                let value = tokens.next().ok_or_else(|| {
                    AppError::BadRequest(format!("'{token}' requires a date YYYY-MM-DD"))
                })?;
                let date = NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| {
                    AppError::BadRequest(format!(
                        "invalid date '{value}', expected format YYYY-MM-DD"
                    ))
                })?;
                if token == "from" {
                    if from.is_some() {
                        return Err(AppError::BadRequest("duplicate 'from'".into()));
                    }
                    from = Some(date);
                } else {
                    if to.is_some() {
                        return Err(AppError::BadRequest("duplicate 'to'".into()));
                    }
                    to = Some(date);
                }
            }
            other => {
                return Err(AppError::BadRequest(format!(
                    "unknown stats option '{other}'"
                )));
            }
        }
    }

    Ok(Command::Stats { view, from, to })
}

pub async fn execute(state: &AppState, user_id: Uuid, command: &Command) -> AppResult<Outcome> {
    match command {
        Command::Upload { path } => {
            let content = std::fs::read_to_string(path)
                .map_err(|e| AppError::BadRequest(format!("cannot read '{path}': {e}")))?;
            let count = expense_service::import_csv(state, user_id, &content).await?;
            Ok(Outcome::Output(format!(
                "Imported {count} expenses from {path}"
            )))
        }
        Command::Stats { view, from, to } => {
            let expenses = expense_service::get_user_expenses(state, user_id, *from, *to).await;
            if expenses.is_empty() {
                return Ok(Outcome::Output(
                    "No expenses found for the selected period.".into(),
                ));
            }
            let rendered = match view {
                None => format!(
                    "{}\n\n{}",
                    views::render_summary(&analytics_service::summary(&expenses)),
                    views::render_categories(&analytics_service::by_category(&expenses))
                ),
                Some(StatsView::Summary) => {
                    views::render_summary(&analytics_service::summary(&expenses))
                }
                Some(StatsView::Category) => {
                    views::render_categories(&analytics_service::by_category(&expenses))
                }
                Some(StatsView::Account) => {
                    views::render_accounts(&analytics_service::by_account(&expenses))
                }
                Some(StatsView::Timeline { group_by }) => {
                    let group_by = match group_by {
                        GroupBy::Day => "day",
                        GroupBy::Week => "week",
                    };
                    views::render_timeline(&analytics_service::timeline(&expenses, group_by))
                }
            };
            Ok(Outcome::Output(rendered))
        }
        Command::Help => Ok(Outcome::Output(views::HELP.to_string())),
        Command::Exit => Ok(Outcome::Exit),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::config::Config;
    use crate::common::models::{Account, Expense};
    use crate::common::services::auth_service;
    use rust_decimal::{dec, Decimal};

    fn test_state() -> AppState {
        AppState::from_config(&Config {
            jwt_secret: "test_secret".into(),
            jwt_expires_in: 3600,
            port: 0,
        })
    }

    async fn registered_state() -> (AppState, Uuid) {
        let state = test_state();
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

    fn make_expense(user_id: Uuid, date: NaiveDate, account: Account, category: &str, amount: Decimal) -> Expense {
        Expense {
            id: Uuid::new_v4(),
            user_id,
            date,
            account,
            category: category.into(),
            amount,
            currency: "BYN".into(),
            description: String::new(),
        }
    }

    #[test]
    fn test_parse_upload() {
        let cmd = parse_command("upload /tmp/my data.csv").unwrap();
        assert!(matches!(cmd, Command::Upload { path } if path == "/tmp/my data.csv"));
    }

    #[test]
    fn test_parse_upload_without_path() {
        assert!(parse_command("upload").is_err());
        assert!(parse_command("upload   ").is_err());
    }

    #[test]
    fn test_parse_stats_default() {
        let cmd = parse_command("stats").unwrap();
        assert!(matches!(cmd, Command::Stats { view: None, from: None, to: None }));
    }

    #[test]
    fn test_parse_stats_views() {
        assert!(matches!(
            parse_command("stats summary").unwrap(),
            Command::Stats { view: Some(StatsView::Summary), .. }
        ));
        assert!(matches!(
            parse_command("stats category").unwrap(),
            Command::Stats { view: Some(StatsView::Category), .. }
        ));
        assert!(matches!(
            parse_command("stats account").unwrap(),
            Command::Stats { view: Some(StatsView::Account), .. }
        ));
    }

    #[test]
    fn test_parse_stats_timeline() {
        assert!(matches!(
            parse_command("stats timeline").unwrap(),
            Command::Stats { view: Some(StatsView::Timeline { group_by: GroupBy::Day }), .. }
        ));
        assert!(matches!(
            parse_command("stats timeline week").unwrap(),
            Command::Stats { view: Some(StatsView::Timeline { group_by: GroupBy::Week }), .. }
        ));
    }

    #[test]
    fn test_parse_stats_dates() {
        let cmd = parse_command("stats from 2026-04-01 to 2026-04-30").unwrap();
        assert!(matches!(
            cmd,
            Command::Stats { view: None, from: Some(f), to: Some(t) }
                if f == NaiveDate::from_ymd_opt(2026, 4, 1).unwrap()
                    && t == NaiveDate::from_ymd_opt(2026, 4, 30).unwrap()
        ));

        let cmd = parse_command("stats summary to 2026-04-30 from 2026-04-01").unwrap();
        assert!(matches!(cmd, Command::Stats { from: Some(_), to: Some(_), .. }));
    }

    #[test]
    fn test_parse_invalid_date() {
        let err = parse_command("stats from 01-04-2026").unwrap_err();
        assert!(err.to_string().contains("invalid date"));
    }

    #[test]
    fn test_parse_from_without_value() {
        let err = parse_command("stats from").unwrap_err();
        assert!(err.to_string().contains("requires a date"));
    }

    #[test]
    fn test_parse_day_without_timeline() {
        assert!(parse_command("stats day").is_err());
        assert!(parse_command("stats week").is_err());
    }

    #[test]
    fn test_parse_duplicate_view() {
        assert!(parse_command("stats summary category").is_err());
        assert!(parse_command("stats timeline summary").is_err());
    }

    #[test]
    fn test_parse_unknown() {
        let err = parse_command("foobar").unwrap_err();
        assert!(err.to_string().contains("unknown command 'foobar'"));
        assert!(parse_command("").is_err());
        assert!(matches!(parse_command("help").unwrap(), Command::Help));
        assert!(matches!(parse_command("exit").unwrap(), Command::Exit));
        assert!(matches!(parse_command("quit").unwrap(), Command::Exit));
    }

    #[tokio::test]
    async fn test_execute_upload_fixture() {
        let (state, user_id) = registered_state().await;
        let cmd = parse_command("upload tests/fixtures/monefy.csv").unwrap();
        match execute(&state, user_id, &cmd).await.unwrap() {
            Outcome::Output(text) => assert!(text.contains("Imported 41 expenses")),
            Outcome::Exit => panic!("expected output"),
        }
    }

    #[tokio::test]
    async fn test_execute_upload_missing_file() {
        let (state, user_id) = registered_state().await;
        let cmd = parse_command("upload /nonexistent/x.csv").unwrap();
        let err = execute(&state, user_id, &cmd).await.unwrap_err();
        assert!(err.to_string().contains("cannot read"));
    }

    #[tokio::test]
    async fn test_execute_stats_empty() {
        let (state, user_id) = registered_state().await;
        let cmd = parse_command("stats").unwrap();
        match execute(&state, user_id, &cmd).await.unwrap() {
            Outcome::Output(text) => {
                assert_eq!(text, "No expenses found for the selected period.")
            }
            Outcome::Exit => panic!("expected output"),
        }
    }

    #[tokio::test]
    async fn test_execute_stats_overview() {
        let (state, user_id) = registered_state().await;
        let mut expenses = state.expenses.write().await;
        expenses.push(make_expense(
            user_id,
            NaiveDate::from_ymd_opt(2026, 4, 22).unwrap(),
            Account::Cash,
            "Food",
            dec!(-50.0),
        ));
        expenses.push(make_expense(
            user_id,
            NaiveDate::from_ymd_opt(2026, 4, 22).unwrap(),
            Account::Cash,
            "Deposits",
            dec!(100.0),
        ));
        expenses.push(make_expense(
            user_id,
            NaiveDate::from_ymd_opt(2026, 5, 1).unwrap(),
            Account::Cash,
            "Food",
            dec!(-30.0),
        ));
        drop(expenses);

        let cmd = parse_command("stats").unwrap();
        match execute(&state, user_id, &cmd).await.unwrap() {
            Outcome::Output(text) => {
                assert!(text.contains("100.00"));
                assert!(text.contains("80.00"));
                assert!(text.contains("20.00"));
                assert!(text.contains("Food"));
            }
            Outcome::Exit => panic!("expected output"),
        }
    }

    #[tokio::test]
    async fn test_execute_stats_date_filter() {
        let (state, user_id) = registered_state().await;
        let mut expenses = state.expenses.write().await;
        expenses.push(make_expense(
            user_id,
            NaiveDate::from_ymd_opt(2026, 4, 22).unwrap(),
            Account::Cash,
            "Food",
            dec!(-50.0),
        ));
        expenses.push(make_expense(
            user_id,
            NaiveDate::from_ymd_opt(2026, 5, 1).unwrap(),
            Account::Cash,
            "Food",
            dec!(-30.0),
        ));
        drop(expenses);

        let cmd = parse_command("stats from 2026-05-01 to 2026-05-31").unwrap();
        match execute(&state, user_id, &cmd).await.unwrap() {
            Outcome::Output(text) => {
                assert!(text.contains("30.00"));
                assert!(!text.contains("50.00"));
            }
            Outcome::Exit => panic!("expected output"),
        }
    }

    #[tokio::test]
    async fn test_execute_help_and_exit() {
        let (state, user_id) = registered_state().await;
        match execute(&state, user_id, &Command::Help).await.unwrap() {
            Outcome::Output(text) => assert!(text.contains("upload")),
            Outcome::Exit => panic!("expected output"),
        }
        assert!(matches!(
            execute(&state, user_id, &Command::Exit).await.unwrap(),
            Outcome::Exit
        ));
    }
}
