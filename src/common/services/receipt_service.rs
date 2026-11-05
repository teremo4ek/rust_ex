use std::io::Write;
use std::process::{Command, Stdio};

use chrono::NaiveDate;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::common::error::{AppError, AppResult};
use crate::common::models::{Account, Expense, TransactionKind};
use crate::common::state::AppState;

use super::expense_service::{self, ImportReport};

const MULTIPLIERS: [&str; 4] = ["х", "Х", "x", "X"];

fn extract_pdf_text(pdf: &[u8]) -> AppResult<String> {
    let mut child = Command::new("pdftotext")
        .args(["-layout", "-", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AppError::Internal(format!("cannot spawn pdftotext: {e}")))?;

    // a broken pipe here means pdftotext rejected the input and exited early
    child
        .stdin
        .take()
        .unwrap()
        .write_all(pdf)
        .map_err(|e| AppError::BadRequest(format!("pdftotext did not accept the file: {e}")))?;

    let output = child
        .wait_with_output()
        .map_err(|e| AppError::Internal(format!("pdftotext wait failed: {e}")))?;

    if !output.status.success() {
        return Err(AppError::BadRequest(format!(
            "pdftotext failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    String::from_utf8(output.stdout)
        .map_err(|_| AppError::BadRequest("pdftotext output is not UTF-8".into()))
}

pub fn parse_receipt(text: &str, user_id: Uuid) -> AppResult<Vec<Expense>> {
    enum State {
        Idle,
        ExpectName,
        ExpectPrice(String),
        ExpectDiscount { name: String, total: Decimal },
    }

    let mut items: Vec<(String, Decimal)> = Vec::new();
    let mut total = None;

    let mut state = State::Idle;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if line.contains("ИТОГО К ОПЛАТЕ") {
            if let State::ExpectDiscount { name, total } = state {
                items.push((name, total));
            }
            let token = line.split_whitespace().next_back().unwrap_or_default();
            total = Some(
                token
                    .parse()
                    .map_err(|e| AppError::BadRequest(format!("cannot parse total '{token}': {e}")))?,
            );
            break;
        }
        state = match state {
            State::Idle => {
                if is_item_code(line) {
                    State::ExpectName
                } else {
                    State::Idle
                }
            }
            State::ExpectName => State::ExpectPrice(line.to_string()),
            State::ExpectPrice(name) => match parse_price_line(line) {
                Some((_, _, line_total)) => State::ExpectDiscount {
                    name,
                    total: line_total,
                },
                None => {
                    return Err(AppError::BadRequest(format!(
                        "cannot parse price line for item '{name}': '{line}'"
                    )))
                }
            },
            State::ExpectDiscount { name, total } => match parse_discount_line(line) {
                Some(discount) => {
                    items.push((name, total + discount));
                    State::Idle
                }
                None => {
                    items.push((name, total));
                    if is_item_code(line) {
                        State::ExpectName
                    } else {
                        State::Idle
                    }
                }
            },
        };
    }

    let Some(total) = total else {
        return Err(AppError::BadRequest("receipt has no total".into()));
    };
    if items.is_empty() {
        return Err(AppError::BadRequest("no items found in receipt".into()));
    }

    let date =
        find_receipt_date(text).ok_or_else(|| AppError::BadRequest("receipt has no date".into()))?;
    let account = if text.contains("КАРТОЙ") {
        Account::PaymentCard
    } else {
        Account::Cash
    };

    let sum: Decimal = items.iter().map(|(_, t)| *t).sum();
    if sum != total {
        return Err(AppError::BadRequest(format!(
            "receipt total mismatch: items sum {sum}, receipt total {total}"
        )));
    }

    Ok(items
        .into_iter()
        .map(|(name, line_total)| Expense {
            id: Uuid::new_v4(),
            user_id,
            date,
            account,
            category: "Groceries".to_string(),
            amount: line_total,
            kind: TransactionKind::Expense,
            currency: "BYN".to_string(),
            amount_usd: None,
            description: name,
        })
        .collect())
}

fn is_item_code(line: &str) -> bool {
    line.bytes().all(|b| b.is_ascii_digit())
}

fn parse_price_line(line: &str) -> Option<(Decimal, Decimal, Decimal)> {
    let toks: Vec<&str> = line.split_whitespace().collect();
    let (price, qty, total) = match toks.as_slice() {
        [p, sep, q, t] if MULTIPLIERS.contains(sep) => (*p, *q, *t),
        [p, q, t] => {
            let qty = MULTIPLIERS.iter().find_map(|m| q.strip_prefix(*m))?;
            (*p, qty, *t)
        }
        _ => return None,
    };
    Some((
        price.parse().ok()?,
        qty.parse().ok()?,
        total.parse().ok()?,
    ))
}

fn parse_discount_line(line: &str) -> Option<Decimal> {
    let mut tokens = line.split_whitespace();
    match (tokens.next(), tokens.next(), tokens.next()) {
        (Some(label), Some(amount), None) if label.to_lowercase() == "скидка" => {
            amount.parse().ok()
        }
        _ => None,
    }
}

fn parse_dash_date(token: &str) -> Option<NaiveDate> {
    let bytes = token.as_bytes();
    if bytes.len() != 10 || bytes[2] != b'-' || bytes[5] != b'-' {
        return None;
    }
    if !bytes
        .iter()
        .enumerate()
        .all(|(i, b)| matches!(i, 2 | 5) || b.is_ascii_digit())
    {
        return None;
    }
    NaiveDate::parse_from_str(token, "%d-%m-%Y").ok()
}

fn find_receipt_date(text: &str) -> Option<NaiveDate> {
    text.lines()
        .find_map(|line| line.split_whitespace().find_map(parse_dash_date))
}

pub async fn import_receipt(
    state: &AppState,
    user_id: Uuid,
    pdf: &[u8],
) -> AppResult<ImportReport> {
    let text = extract_pdf_text(pdf)?;
    let expenses = parse_receipt(&text, user_id)?;
    let mut tx = state.pool.begin().await?;
    for expense in &expenses {
        expense_service::insert_expense(&mut tx, expense).await?;
    }
    tx.commit().await?;
    Ok(ImportReport {
        imported: expenses.len(),
        duplicates: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::config::Config;
    use crate::common::models::dto::ExpensesQuery;
    use crate::common::services::auth_service;
    use crate::common::services::expense_service;
    use rust_decimal::dec;

    const RECEIPT_TEXT: &str = r#"                           ООО "ЕВРОТОРГ"
  ООО "ЕВРОТОРГ", МАГАЗИН "ЕВРООПТ", Г.МИНСК,
            УЛ.ПРИМЕРНАЯ, 1
             ДОБРО ПОЖАЛОВАТЬ!
                             УНП 100000001
                          РН СККО 100000002
 --------------------------------------------------------------------------------
Платежный документ                                              N док. 00001
1000001
Яблоко РБ 1кг
1.16                                х 1.072                                  1.24


2000002
Риет-салат "САМОЕ ВРЕМЯ"(ветч/сыр)140г
2.99                       х1                                                2.99


3000003
Хлопья "ГЕРКУЛЕС"(овс.,монастыр.) 500г
3.99                         х1                                              3.99


4000004
Мак.изд. "BONERO" (спагетти) 400г
3.29                          х1                                             3.29


 --------------------------------------------------------------------------------
ИТОГО К ОПЛАТЕ                                                            11.51
ПЛ. КАРТОЙ                                                                11.51
 --------------------------------------------------------------------------------
Кассир: Иванова Анна Петровна
02-11-2026 18:34                                            700-00-4000000

               УИ: 0000000000000000000000




 --------------------------------------------------------------------------------
Карта "Еплюс"
1 БОНУС = 1 КОПЕЙКА
Номер карты:                                               *********00000***
Начислено бонусов:                                             5.50(0.05руб)
Списано бонусов:                                                            0.00
 --------------------------------------------------------------------------------
                            ООО ЕВРОТОРГ
                       магазин =Euroopt PRIME=
                   г. Минск, ул. Примерная, 1
                         Терминал:TERMIN01
                           КАРТ-ЧЕК:00000
                            *ДЛЯ КЛИЕНТА*
                              ОПЛАТА
               02.11.2026                         18:34:39
               КАРТА:**** **** **** 0000
               VISA DEBIT payWaveApp
               A0000000000000
               TC:0000000000000000
               TVR:0000000000                          TSI:0000
               Сумма операции:
                                           11,51 BYN
               RRN:000000000000                      КОД АВТ:000000
               Чек 0000000/UN=0000000
                             КОД:000
                             ОДОБРЕНО
               Личность подтверждена на мобильном
               устройстве.
               ------------------------------------------"#;

    const DISCOUNT_RECEIPT_TEXT: &str = r#"                           ООО "ЕВРОТОРГ"
  ООО "ЕВРОТОРГ", МАГАЗИН "ЕВРООПТ", Г.МИНСК,
            УЛ.ПРИМЕРНАЯ, 1
             ДОБРО ПОЖАЛОВАТЬ!
                             УНП 100000001
                          РН СККО 100000002
 --------------------------------------------------------------------------------
Платежный документ                                              N док. 00002
1000001
[M] Сыр твор"БЕЛЫЙ САЛАТНЫЙ"(слив50%)Сав100г
1.79                      х1                                                 1.79
Скидка                                                                      -0.06

2000002
Лук репчатый (урожай 2026) РБ 1кг
1.95                        х 0.598                                          1.17
Скидка                                                                      -0.05

3000003
Картофель (урожай 2026) РБ 1кг
1.32                       х 1.284                                           1.69
Скидка                                                                      -0.05

4000004
[M] Сметана"МИНСКАЯ МАРКА"(пленка, 15%) 400г
2.64                      х1                                                 2.64
Скидка                                                                      -0.08

5000005
[M] Морс "RICH" (клюква,виноград) 1л
4.99                          х1                                             4.99
Скидка                                                                      -0.15

6000006
[M] Молоко"ОТБОРНОЕ"(3.4-6%пэт)Ясь Бел 930мл
2.92                       х1                                                2.92
Скидка                                                                      -0.08

 --------------------------------------------------------------------------------
ИТОГО К ОПЛАТЕ                                                            14.73
ПЛ. КАРТОЙ                                                                14.73
ИТОГО СКИДКА                                                                0.47
 --------------------------------------------------------------------------------
Кассир: Иванова Анна Петровна
08-08-2026 19:46                                            700-00-4000000

                УИ: 000000000000000000000000




 --------------------------------------------------------------------------------
Карта "Еплюс"
1 БОНУС = 1 КОПЕЙКА
Номер карты:                                               *********00000***
Начислено бонусов:                                             7.00(0.07руб)
Списано бонусов:                                             47.00(0.47руб)
 --------------------------------------------------------------------------------
                           ООО ЕВРОТОРГ
                      магазин =Euroopt PRIME=
                   г. Минск, ул. Примерная, 1
                        Терминал:TERMIN01
                          КАРТ-ЧЕК:00000
                           *ДЛЯ КЛИЕНТА*
                             ОПЛАТА
               08.08.2026                        19:46:47
               КАРТА:**** **** **** 0000
               VISA DEBIT payWaveApp
               0000000000000
               TC:0000000000000000
               TVR:0000000000                         TSI:0000
               Сумма операции:
                                         14,73 BYN
               RRN:000000000000                    КОД АВТ:000000
               Чек 0000000/UN=0000000
                             КОД:000
                             ОДОБРЕНО
               Личность подтверждена на мобильном
               устройстве.
               ------------------------------------------"#;

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

    fn map_line_containing(receipt: &str, needle: &str, f: impl Fn(&str) -> String) -> String {
        receipt
            .lines()
            .map(|l| {
                if l.contains(needle) {
                    f(l)
                } else {
                    l.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn test_parse_receipt_extracts_items() {
        let expenses = parse_receipt(RECEIPT_TEXT, Uuid::new_v4()).unwrap();
        assert_eq!(expenses.len(), 4);
        let amounts: Vec<Decimal> = expenses.iter().map(|e| e.amount).collect();
        assert_eq!(
            amounts,
            vec![dec!(1.24), dec!(2.99), dec!(3.99), dec!(3.29)]
        );
        assert_eq!(expenses[0].description, "Яблоко РБ 1кг");
        assert_eq!(
            expenses[3].description,
            "Мак.изд. \"BONERO\" (спагетти) 400г"
        );
        assert!(expenses.iter().all(|e| e.category == "Groceries"));
        assert!(expenses.iter().all(|e| e.kind == TransactionKind::Expense));
        assert!(expenses.iter().all(|e| e.currency == "BYN"));
        assert!(expenses.iter().all(|e| e.account == Account::PaymentCard));
        assert!(
            expenses
                .iter()
                .all(|e| e.date == NaiveDate::from_ymd_opt(2026, 11, 2).unwrap())
        );
        assert!(expenses.iter().all(|e| e.amount_usd.is_none()));
    }

    #[test]
    fn test_parse_receipt_defaults_to_cash_without_card_line() {
        let text = RECEIPT_TEXT.replace("ПЛ. КАРТОЙ", "ПЛ. НАЛИЧНЫМИ");
        let expenses = parse_receipt(&text, Uuid::new_v4()).unwrap();
        assert!(expenses.iter().all(|e| e.account == Account::Cash));
    }

    #[test]
    fn test_parse_receipt_with_discounts() {
        let expenses = parse_receipt(DISCOUNT_RECEIPT_TEXT, Uuid::new_v4()).unwrap();
        assert_eq!(expenses.len(), 6);
        let amounts: Vec<Decimal> = expenses.iter().map(|e| e.amount).collect();
        assert_eq!(
            amounts,
            vec![
                dec!(1.73),
                dec!(1.12),
                dec!(1.64),
                dec!(2.56),
                dec!(4.84),
                dec!(2.84)
            ]
        );
        assert!(expenses.iter().all(|e| e.account == Account::PaymentCard));
        assert!(
            expenses
                .iter()
                .all(|e| e.date == NaiveDate::from_ymd_opt(2026, 8, 8).unwrap())
        );
    }

    #[test]
    fn test_parse_receipt_last_item_flushed_at_total() {
        let text = "1000001\nЯблоко РБ 1кг\n1.00            х1            1.00\n\nИТОГО К ОПЛАТЕ                                                            1.00\n02-11-2026 18:34\n";
        let expenses = parse_receipt(text, Uuid::new_v4()).unwrap();
        assert_eq!(expenses.len(), 1);
        assert_eq!(expenses[0].amount, dec!(1));
        assert_eq!(expenses[0].description, "Яблоко РБ 1кг");
    }

    #[test]
    fn test_parse_receipt_total_mismatch_is_error() {
        let text = map_line_containing(RECEIPT_TEXT, "ИТОГО К ОПЛАТЕ", |l| {
            l.replacen("11.51", "11.52", 1)
        });
        let err = parse_receipt(&text, Uuid::new_v4()).unwrap_err();
        assert!(err.to_string().contains("total mismatch"));
    }

    #[test]
    fn test_parse_receipt_missing_total_is_error() {
        let text = RECEIPT_TEXT
            .lines()
            .filter(|l| !l.contains("ИТОГО"))
            .collect::<Vec<_>>()
            .join("\n");
        let err = parse_receipt(&text, Uuid::new_v4()).unwrap_err();
        assert!(err.to_string().contains("no total"));
    }

    #[test]
    fn test_parse_receipt_missing_date_is_error() {
        let text = RECEIPT_TEXT
            .lines()
            .filter(|l| !l.contains("02-11-2026"))
            .collect::<Vec<_>>()
            .join("\n");
        let err = parse_receipt(&text, Uuid::new_v4()).unwrap_err();
        assert!(err.to_string().contains("no date"));
    }

    #[test]
    fn test_parse_receipt_no_items_is_error() {
        let text = "ООО \"ЕВРОТОРГ\"\nИТОГО К ОПЛАТЕ                    11.51\n02-11-2026 18:34\n";
        let err = parse_receipt(text, Uuid::new_v4()).unwrap_err();
        assert!(err.to_string().contains("no items"));
    }

    #[test]
    fn test_parse_receipt_bad_price_line_names_item() {
        let text = map_line_containing(RECEIPT_TEXT, "х1", |l| l.replacen("х1", "z1", 1));
        let err = parse_receipt(&text, Uuid::new_v4()).unwrap_err();
        assert!(err.to_string().contains("Риет-салат"));
    }

    #[test]
    fn test_extract_pdf_text_fixture_parses_receipt() {
        let pdf = include_bytes!("../../../tests/fixtures/check.pdf");
        let text = extract_pdf_text(pdf).unwrap();
        let expenses = parse_receipt(&text, Uuid::new_v4()).unwrap();
        assert_eq!(expenses.len(), 4);
        let sum: Decimal = expenses.iter().map(|e| e.amount).sum();
        assert_eq!(sum, dec!(11.51));
    }

    #[test]
    fn test_extract_pdf_text_fixture_parses_receipt_with_discounts() {
        let pdf = include_bytes!("../../../tests/fixtures/check2.pdf");
        let text = extract_pdf_text(pdf).unwrap();
        let expenses = parse_receipt(&text, Uuid::new_v4()).unwrap();
        assert_eq!(expenses.len(), 6);
        let sum: Decimal = expenses.iter().map(|e| e.amount).sum();
        assert_eq!(sum, dec!(14.73));
    }

    #[tokio::test]
    async fn test_import_receipt_inserts_rows() {
        let (state, user_id) = registered_state().await;
        let pdf = include_bytes!("../../../tests/fixtures/check.pdf");
        let report = import_receipt(&state, user_id, pdf).await.unwrap();
        assert_eq!(
            report,
            ImportReport {
                imported: 4,
                duplicates: 0
            }
        );
        assert_eq!(
            expense_service::count_expenses(&state, user_id, &empty_query())
                .await
                .unwrap(),
            4
        );
    }

    #[tokio::test]
    async fn test_import_receipt_rejects_non_pdf() {
        let (state, user_id) = registered_state().await;
        let err = import_receipt(&state, user_id, b"definitely not a pdf")
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }
}
