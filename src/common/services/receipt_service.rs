use std::io::Write;
use std::process::{Command, Stdio};

use chrono::NaiveDate;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::common::error::{AppError, AppResult};
use crate::common::models::receipt::{
    ParsedItem, ParsedReceipt, Receipt, ReceiptItem, ReceiptReport, ReceiptStatus,
};
use crate::common::models::Account;
use crate::common::state::AppState;

const MULTIPLIERS: [&str; 4] = ["х", "Х", "x", "X"];
const CURRENCY: &str = "BYN";

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

struct PricedItem {
    code: String,
    name: String,
    unit_price: Decimal,
    quantity: Decimal,
    line_total: Decimal,
}

impl PricedItem {
    fn finish(self, discount: Decimal) -> ParsedItem {
        ParsedItem {
            code: self.code,
            name: self.name,
            unit_price: self.unit_price,
            quantity: self.quantity,
            discount,
            total: self.line_total + discount,
        }
    }
}

pub fn parse_receipt(text: &str) -> AppResult<ParsedReceipt> {
    enum State {
        Idle,
        ExpectName(String),
        ExpectPrice { code: String, name: String },
        ExpectDiscount(PricedItem),
    }

    let mut items: Vec<ParsedItem> = Vec::new();
    let mut total = None;

    let mut state = State::Idle;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if line.contains("ИТОГО К ОПЛАТЕ") {
            if let State::ExpectDiscount(item) = state {
                items.push(item.finish(Decimal::ZERO));
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
                    State::ExpectName(line.to_string())
                } else {
                    State::Idle
                }
            }
            State::ExpectName(code) => State::ExpectPrice {
                code,
                name: line.to_string(),
            },
            State::ExpectPrice { code, name } => match parse_price_line(line) {
                Some((unit_price, quantity, line_total)) => {
                    State::ExpectDiscount(PricedItem {
                        code,
                        name,
                        unit_price,
                        quantity,
                        line_total,
                    })
                }
                None => {
                    return Err(AppError::BadRequest(format!(
                        "cannot parse price line for item '{name}': '{line}'"
                    )))
                }
            },
            State::ExpectDiscount(item) => match parse_discount_line(line) {
                Some(discount) => {
                    items.push(item.finish(discount));
                    State::Idle
                }
                None => {
                    items.push(item.finish(Decimal::ZERO));
                    if is_item_code(line) {
                        State::ExpectName(line.to_string())
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
    let fiscal_id = find_receipt_uid(text)
        .ok_or_else(|| AppError::BadRequest("receipt has no unique id".into()))?;
    let account = if text.contains("КАРТОЙ") {
        Account::PaymentCard
    } else {
        Account::Cash
    };

    let sum: Decimal = items.iter().map(|i| i.total).sum();
    if sum != total {
        return Err(AppError::BadRequest(format!(
            "receipt total mismatch: items sum {sum}, receipt total {total}"
        )));
    }

    Ok(ParsedReceipt {
        date,
        account,
        total,
        fiscal_id,
        items,
    })
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

fn find_receipt_uid(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let trimmed = line.trim();
        trimmed
            .strip_prefix("УИ:")
            .map(|rest| rest.trim().to_string())
            .filter(|id| !id.is_empty())
    })
}

enum AttachOutcome {
    Attached(Uuid),
    NoMatch,
}

async fn try_attach(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    user_id: Uuid,
    receipt_id: Uuid,
    date: NaiveDate,
    total: Decimal,
) -> AppResult<AttachOutcome> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, amount FROM expenses
         WHERE user_id = ? AND date = ? AND kind = 'expense' AND currency = 'BYN'
           AND NOT EXISTS (SELECT 1 FROM receipts r WHERE r.expense_id = expenses.id)
         ORDER BY rowid",
    )
    .bind(user_id.to_string())
    .bind(date)
    .fetch_all(&mut **tx)
    .await?;

    for (id, amount) in rows {
        let amount: Decimal = amount
            .parse()
            .map_err(|_| AppError::Database("invalid amount stored in expenses".into()))?;
        if amount != total {
            continue;
        }
        let expense_id: Uuid = id
            .parse()
            .map_err(|_| AppError::Database("invalid expense id stored in expenses".into()))?;
        sqlx::query("UPDATE receipts SET expense_id = ? WHERE id = ?")
            .bind(expense_id.to_string())
            .bind(receipt_id.to_string())
            .execute(&mut **tx)
            .await?;
        return Ok(AttachOutcome::Attached(expense_id));
    }
    Ok(AttachOutcome::NoMatch)
}

async fn insert_receipt(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    receipt_id: Uuid,
    user_id: Uuid,
    parsed: &ParsedReceipt,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO receipts (id, user_id, date, total, currency, account, fiscal_id)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(receipt_id.to_string())
    .bind(user_id.to_string())
    .bind(parsed.date)
    .bind(parsed.total.to_string())
    .bind(CURRENCY)
    .bind(parsed.account.to_string())
    .bind(parsed.fiscal_id.clone())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn insert_receipt_item(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    receipt_id: Uuid,
    position: i64,
    item: &ParsedItem,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO receipt_items (id, receipt_id, position, code, name, unit_price, quantity, discount, total)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(receipt_id.to_string())
    .bind(position)
    .bind(item.code.clone())
    .bind(item.name.clone())
    .bind(item.unit_price.to_string())
    .bind(item.quantity.to_string())
    .bind(item.discount.to_string())
    .bind(item.total.to_string())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

const RECEIPT_COLUMNS: &str = "id, user_id, expense_id, date, total, currency, account, fiscal_id";

#[derive(Debug, sqlx::FromRow)]
struct ReceiptRow {
    id: String,
    user_id: String,
    expense_id: Option<String>,
    date: NaiveDate,
    total: String,
    currency: String,
    account: String,
    fiscal_id: String,
}

impl ReceiptRow {
    fn into_receipt(self) -> AppResult<Receipt> {
        Ok(Receipt {
            id: self
                .id
                .parse()
                .map_err(|_| AppError::Database("invalid receipt id stored in receipts".into()))?,
            user_id: self
                .user_id
                .parse()
                .map_err(|_| AppError::Database("invalid user id stored in receipts".into()))?,
            expense_id: self
                .expense_id
                .map(|v| v.parse::<Uuid>())
                .transpose()
                .map_err(|_| {
                    AppError::Database("invalid expense id stored in receipts".into())
                })?,
            date: self.date,
            total: self
                .total
                .parse()
                .map_err(|_| AppError::Database("invalid total stored in receipts".into()))?,
            currency: self.currency,
            account: self
                .account
                .parse()
                .map_err(|_| AppError::Database("invalid account stored in receipts".into()))?,
            fiscal_id: self.fiscal_id,
            items: Vec::new(),
        })
    }
}

#[derive(Debug, sqlx::FromRow)]
struct ReceiptItemRow {
    receipt_id: String,
    position: i64,
    code: String,
    name: String,
    unit_price: String,
    quantity: String,
    discount: String,
    total: String,
}

impl ReceiptItemRow {
    fn into_receipt_item(self) -> AppResult<ReceiptItem> {
        let bad = |field: &str| {
            AppError::Database(format!("invalid {field} stored in receipt_items"))
        };
        Ok(ReceiptItem {
            receipt_id: self
                .receipt_id
                .parse()
                .map_err(|_| bad("receipt id"))?,
            position: self.position,
            code: self.code,
            name: self.name,
            unit_price: self.unit_price.parse().map_err(|_| bad("unit price"))?,
            quantity: self.quantity.parse().map_err(|_| bad("quantity"))?,
            discount: self.discount.parse().map_err(|_| bad("discount"))?,
            total: self.total.parse().map_err(|_| bad("total"))?,
        })
    }
}

async fn load_items(
    pool: &sqlx::Pool<sqlx::Sqlite>,
    receipt_id: Uuid,
) -> AppResult<Vec<ReceiptItem>> {
    let rows: Vec<ReceiptItemRow> = sqlx::query_as(
        "SELECT receipt_id, position, code, name, unit_price, quantity, discount, total
         FROM receipt_items WHERE receipt_id = ? ORDER BY position",
    )
    .bind(receipt_id.to_string())
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(ReceiptItemRow::into_receipt_item).collect()
}

async fn row_with_items(state: &AppState, row: ReceiptRow) -> AppResult<Receipt> {
    let mut receipt = row.into_receipt()?;
    receipt.items = load_items(&state.pool, receipt.id).await?;
    Ok(receipt)
}

async fn find_by_fiscal_id(
    state: &AppState,
    user_id: Uuid,
    fiscal_id: &str,
) -> AppResult<Option<Receipt>> {
    let row: Option<ReceiptRow> =
        sqlx::query_as(&format!(
            "SELECT {RECEIPT_COLUMNS} FROM receipts WHERE user_id = ? AND fiscal_id = ?"
        ))
        .bind(user_id.to_string())
        .bind(fiscal_id)
        .fetch_optional(&state.pool)
        .await?;
    match row {
        Some(row) => Ok(Some(row_with_items(state, row).await?)),
        None => Ok(None),
    }
}

pub async fn import_receipt(
    state: &AppState,
    user_id: Uuid,
    pdf: &[u8],
) -> AppResult<ReceiptReport> {
    let text = extract_pdf_text(pdf)?;
    let parsed = parse_receipt(&text)?;

    if let Some(existing) = find_by_fiscal_id(state, user_id, &parsed.fiscal_id).await? {
        return Ok(ReceiptReport {
            status: ReceiptStatus::Duplicate,
            date: existing.date,
            total: existing.total,
            currency: existing.currency,
            item_count: existing.items.len(),
            expense_id: existing.expense_id,
        });
    }

    let receipt_id = Uuid::new_v4();
    let mut tx = state.pool.begin().await?;
    insert_receipt(&mut tx, receipt_id, user_id, &parsed).await?;
    for (position, item) in parsed.items.iter().enumerate() {
        insert_receipt_item(&mut tx, receipt_id, position as i64, item).await?;
    }
    let outcome = try_attach(&mut tx, user_id, receipt_id, parsed.date, parsed.total).await?;
    tx.commit().await?;

    let (status, expense_id) = match outcome {
        AttachOutcome::Attached(expense_id) => (ReceiptStatus::Attached, Some(expense_id)),
        AttachOutcome::NoMatch => (ReceiptStatus::Pending, None),
    };
    Ok(ReceiptReport {
        status,
        date: parsed.date,
        total: parsed.total,
        currency: CURRENCY.to_string(),
        item_count: parsed.items.len(),
        expense_id,
    })
}

pub async fn attach_pending_receipts(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    user_id: Uuid,
) -> AppResult<usize> {
    let rows: Vec<(String, NaiveDate, String)> = sqlx::query_as(
        "SELECT id, date, total FROM receipts WHERE user_id = ? AND expense_id IS NULL ORDER BY rowid",
    )
    .bind(user_id.to_string())
    .fetch_all(&mut **tx)
    .await?;

    let mut attached = 0;
    for (id, date, total) in rows {
        let receipt_id: Uuid = id
            .parse()
            .map_err(|_| AppError::Database("invalid receipt id stored in receipts".into()))?;
        let total: Decimal = total
            .parse()
            .map_err(|_| AppError::Database("invalid total stored in receipts".into()))?;
        if let AttachOutcome::Attached(_) = try_attach(tx, user_id, receipt_id, date, total).await? {
            attached += 1;
        }
    }
    Ok(attached)
}

pub async fn get_expense_receipt(
    state: &AppState,
    user_id: Uuid,
    expense_id: Uuid,
) -> AppResult<Receipt> {
    let row: Option<ReceiptRow> =
        sqlx::query_as(&format!(
            "SELECT {RECEIPT_COLUMNS} FROM receipts WHERE user_id = ? AND expense_id = ?"
        ))
        .bind(user_id.to_string())
        .bind(expense_id.to_string())
        .fetch_optional(&state.pool)
        .await?;
    match row {
        Some(row) => Ok(row_with_items(state, row).await?),
        None => Err(AppError::NotFound("receipt not found".into())),
    }
}

pub async fn list_pending_receipts(state: &AppState, user_id: Uuid) -> AppResult<Vec<Receipt>> {
    let rows: Vec<ReceiptRow> = sqlx::query_as(&format!(
        "SELECT {RECEIPT_COLUMNS} FROM receipts WHERE user_id = ? AND expense_id IS NULL ORDER BY rowid"
    ))
    .bind(user_id.to_string())
    .fetch_all(&state.pool)
    .await?;

    let mut receipts = Vec::with_capacity(rows.len());
    for row in rows {
        receipts.push(row_with_items(state, row).await?);
    }
    Ok(receipts)
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

               УИ: 000000000000000000000001




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
2.92                       х1                                                 2.92
Скидка                                                                      -0.08

 --------------------------------------------------------------------------------
ИТОГО К ОПЛАТЕ                                                            14.73
ПЛ. КАРТОЙ                                                                14.73
ИТОГО СКИДКА                                                                0.47
 --------------------------------------------------------------------------------
Кассир: Иванова Анна Петровна
08-08-2026 19:46                                            700-00-4000000

                УИ: 000000000000000000000002




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

    const MATCH_CSV: &str = "\
date,account,category,amount,currency,converted amount,currency,description
11/2/2026,Payment card,Food,-11.51,BYN,-11.51,BYN,Euroopt
";

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

    async fn count_receipts(state: &AppState) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM receipts")
            .fetch_one(&state.pool)
            .await
            .unwrap()
    }

    #[test]
    fn test_parse_receipt_extracts_items() {
        let receipt = parse_receipt(RECEIPT_TEXT).unwrap();
        assert_eq!(receipt.items.len(), 4);
        let codes: Vec<&str> = receipt.items.iter().map(|i| i.code.as_str()).collect();
        assert_eq!(codes, vec!["1000001", "2000002", "3000003", "4000004"]);
        let totals: Vec<Decimal> = receipt.items.iter().map(|i| i.total).collect();
        assert_eq!(
            totals,
            vec![dec!(1.24), dec!(2.99), dec!(3.99), dec!(3.29)]
        );
        assert_eq!(receipt.items[0].name, "Яблоко РБ 1кг");
        assert_eq!(
            receipt.items[3].name,
            "Мак.изд. \"BONERO\" (спагетти) 400г"
        );
        assert_eq!(receipt.items[0].unit_price, dec!(1.16));
        assert_eq!(receipt.items[0].quantity, dec!(1.072));
        assert_eq!(receipt.items[1].quantity, dec!(1));
        assert!(receipt.items.iter().all(|i| i.discount == Decimal::ZERO));
        assert_eq!(receipt.total, dec!(11.51));
        assert_eq!(receipt.date, NaiveDate::from_ymd_opt(2026, 11, 2).unwrap());
        assert_eq!(receipt.account, Account::PaymentCard);
        assert_eq!(receipt.fiscal_id, "000000000000000000000001");
    }

    #[test]
    fn test_parse_receipt_defaults_to_cash_without_card_line() {
        let text = RECEIPT_TEXT.replace("ПЛ. КАРТОЙ", "ПЛ. НАЛИЧНЫМИ");
        let receipt = parse_receipt(&text).unwrap();
        assert_eq!(receipt.account, Account::Cash);
    }

    #[test]
    fn test_parse_receipt_with_discounts() {
        let receipt = parse_receipt(DISCOUNT_RECEIPT_TEXT).unwrap();
        assert_eq!(receipt.items.len(), 6);
        let totals: Vec<Decimal> = receipt.items.iter().map(|i| i.total).collect();
        assert_eq!(
            totals,
            vec![
                dec!(1.73),
                dec!(1.12),
                dec!(1.64),
                dec!(2.56),
                dec!(4.84),
                dec!(2.84)
            ]
        );
        let discounts: Vec<Decimal> = receipt.items.iter().map(|i| i.discount).collect();
        assert_eq!(
            discounts,
            vec![
                dec!(-0.06),
                dec!(-0.05),
                dec!(-0.05),
                dec!(-0.08),
                dec!(-0.15),
                dec!(-0.08)
            ]
        );
        assert_eq!(receipt.items[0].unit_price, dec!(1.79));
        assert_eq!(receipt.items[1].quantity, dec!(0.598));
        assert_eq!(receipt.total, dec!(14.73));
        assert_eq!(receipt.account, Account::PaymentCard);
        assert_eq!(receipt.date, NaiveDate::from_ymd_opt(2026, 8, 8).unwrap());
        assert_eq!(receipt.fiscal_id, "000000000000000000000002");
    }

    #[test]
    fn test_parse_receipt_last_item_flushed_at_total() {
        let text = "1000001\nЯблоко РБ 1кг\n1.00            х1            1.00\n\nИТОГО К ОПЛАТЕ                                                            1.00\n02-11-2026 18:34\nУИ: 000000000000000000000003\n";
        let receipt = parse_receipt(text).unwrap();
        assert_eq!(receipt.items.len(), 1);
        assert_eq!(receipt.items[0].total, dec!(1));
        assert_eq!(receipt.items[0].name, "Яблоко РБ 1кг");
        assert_eq!(receipt.items[0].discount, Decimal::ZERO);
    }

    #[test]
    fn test_parse_receipt_total_mismatch_is_error() {
        let text = map_line_containing(RECEIPT_TEXT, "ИТОГО К ОПЛАТЕ", |l| {
            l.replacen("11.51", "11.52", 1)
        });
        let err = parse_receipt(&text).unwrap_err();
        assert!(err.to_string().contains("total mismatch"));
    }

    #[test]
    fn test_parse_receipt_missing_total_is_error() {
        let text = RECEIPT_TEXT
            .lines()
            .filter(|l| !l.contains("ИТОГО"))
            .collect::<Vec<_>>()
            .join("\n");
        let err = parse_receipt(&text).unwrap_err();
        assert!(err.to_string().contains("no total"));
    }

    #[test]
    fn test_parse_receipt_missing_date_is_error() {
        let text = RECEIPT_TEXT
            .lines()
            .filter(|l| !l.contains("02-11-2026"))
            .collect::<Vec<_>>()
            .join("\n");
        let err = parse_receipt(&text).unwrap_err();
        assert!(err.to_string().contains("no date"));
    }

    #[test]
    fn test_parse_receipt_missing_uid_is_error() {
        let text = RECEIPT_TEXT
            .lines()
            .filter(|l| !l.contains("УИ:"))
            .collect::<Vec<_>>()
            .join("\n");
        let err = parse_receipt(&text).unwrap_err();
        assert!(err.to_string().contains("no unique id"));
    }

    #[test]
    fn test_parse_receipt_no_items_is_error() {
        let text = "ООО \"ЕВРОТОРГ\"\nИТОГО К ОПЛАТЕ                    11.51\n02-11-2026 18:34\nУИ: 000000000000000000000004\n";
        let err = parse_receipt(text).unwrap_err();
        assert!(err.to_string().contains("no items"));
    }

    #[test]
    fn test_parse_receipt_bad_price_line_names_item() {
        let text = map_line_containing(RECEIPT_TEXT, "х1", |l| l.replacen("х1", "z1", 1));
        let err = parse_receipt(&text).unwrap_err();
        assert!(err.to_string().contains("Риет-салат"));
    }

    #[test]
    fn test_extract_pdf_text_fixture_parses_receipt() {
        let pdf = include_bytes!("../../../tests/fixtures/check.pdf");
        let text = extract_pdf_text(pdf).unwrap();
        let receipt = parse_receipt(&text).unwrap();
        assert_eq!(receipt.items.len(), 4);
        let sum: Decimal = receipt.items.iter().map(|i| i.total).sum();
        assert_eq!(sum, dec!(11.51));
        assert_eq!(receipt.fiscal_id, "EDDA8397564658AE0718A4AB");
    }

    #[test]
    fn test_extract_pdf_text_fixture_parses_receipt_with_discounts() {
        let pdf = include_bytes!("../../../tests/fixtures/check2.pdf");
        let text = extract_pdf_text(pdf).unwrap();
        let receipt = parse_receipt(&text).unwrap();
        assert_eq!(receipt.items.len(), 6);
        let sum: Decimal = receipt.items.iter().map(|i| i.total).sum();
        assert_eq!(sum, dec!(14.73));
        assert_eq!(receipt.fiscal_id, "BD79D40E63885B830718A4B2");
    }

    #[tokio::test]
    async fn test_import_receipt_without_expense_is_pending() {
        let (state, user_id) = registered_state().await;
        let pdf = include_bytes!("../../../tests/fixtures/check.pdf");
        let report = import_receipt(&state, user_id, pdf).await.unwrap();
        assert_eq!(
            report,
            ReceiptReport {
                status: ReceiptStatus::Pending,
                date: NaiveDate::from_ymd_opt(2026, 11, 2).unwrap(),
                total: dec!(11.51),
                currency: "BYN".into(),
                item_count: 4,
                expense_id: None,
            }
        );
        assert_eq!(
            expense_service::count_expenses(&state, user_id, &empty_query())
                .await
                .unwrap(),
            0
        );
        let pending = list_pending_receipts(&state, user_id).await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].items.len(), 4);
        let sum: Decimal = pending[0].items.iter().map(|i| i.total).sum();
        assert_eq!(sum, dec!(11.51));
    }

    #[tokio::test]
    async fn test_import_receipt_attaches_to_matching_expense() {
        let (state, user_id) = registered_state().await;
        expense_service::import_csv(&state, user_id, MATCH_CSV)
            .await
            .unwrap();
        let pdf = include_bytes!("../../../tests/fixtures/check.pdf");
        let report = import_receipt(&state, user_id, pdf).await.unwrap();
        assert_eq!(report.status, ReceiptStatus::Attached);
        let expense_id = report.expense_id.unwrap();
        assert_eq!(
            expense_service::count_expenses(&state, user_id, &empty_query())
                .await
                .unwrap(),
            1
        );
        let receipt = get_expense_receipt(&state, user_id, expense_id)
            .await
            .unwrap();
        assert_eq!(receipt.items.len(), 4);
        assert_eq!(receipt.items[0].name, "Яблоко РБ 1кг");
        assert_eq!(receipt.items[0].unit_price, dec!(1.16));
        assert_eq!(receipt.items[0].quantity, dec!(1.072));
        assert!(list_pending_receipts(&state, user_id)
            .await
            .unwrap()
            .is_empty());
    }

    #[test]
    fn test_parse_receipt_amount_scale_variant_matches() {
        let receipt = parse_receipt(RECEIPT_TEXT).unwrap();
        assert_eq!(receipt.total, dec!(11.510));
    }

    #[tokio::test]
    async fn test_import_receipt_duplicate_fiscal_id() {
        let (state, user_id) = registered_state().await;
        let pdf = include_bytes!("../../../tests/fixtures/check.pdf");
        let first = import_receipt(&state, user_id, pdf).await.unwrap();
        let second = import_receipt(&state, user_id, pdf).await.unwrap();
        assert_eq!(first.status, ReceiptStatus::Pending);
        assert_eq!(second.status, ReceiptStatus::Duplicate);
        assert_eq!(second.item_count, 4);
        assert_eq!(count_receipts(&state).await, 1);
    }

    #[tokio::test]
    async fn test_import_receipt_ignores_income_and_non_byn() {
        let (state, user_id) = registered_state().await;
        let csv = "\
date,account,category,amount,currency,converted amount,currency,description
11/2/2026,Payment card,Food,11.51,BYN,11.51,BYN,Refund
11/2/2026,Payment card,Food,-11.51,USD,-11.51,USD,Euroopt
";
        expense_service::import_csv(&state, user_id, csv)
            .await
            .unwrap();
        let pdf = include_bytes!("../../../tests/fixtures/check.pdf");
        let report = import_receipt(&state, user_id, pdf).await.unwrap();
        assert_eq!(report.status, ReceiptStatus::Pending);
        assert_eq!(count_receipts(&state).await, 1);
    }

    #[tokio::test]
    async fn test_attach_pending_skips_expenses_with_receipt() {
        let (state, user_id) = registered_state().await;
        expense_service::import_csv(&state, user_id, MATCH_CSV)
            .await
            .unwrap();
        let pdf = include_bytes!("../../../tests/fixtures/check.pdf");
        import_receipt(&state, user_id, pdf).await.unwrap();

        sqlx::query(
            "INSERT INTO receipts (id, user_id, date, total, currency, account, fiscal_id)
             VALUES (?, ?, ?, ?, 'BYN', ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(user_id.to_string())
        .bind(NaiveDate::from_ymd_opt(2026, 11, 2).unwrap())
        .bind("11.51")
        .bind("Payment card")
        .bind("000000000000000000000009")
        .execute(&state.pool)
        .await
        .unwrap();

        let extra_csv = "\
date,account,category,amount,currency,converted amount,currency,description
11/3/2026,Payment card,Transport,-2,BYN,-2,BYN,Metro
";
        let report = expense_service::import_csv(&state, user_id, extra_csv)
            .await
            .unwrap();
        assert_eq!(report.receipts_attached, 0);
        assert_eq!(list_pending_receipts(&state, user_id).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_delete_expense_reverts_receipt_to_pending() {
        let (state, user_id) = registered_state().await;
        expense_service::import_csv(&state, user_id, MATCH_CSV)
            .await
            .unwrap();
        let pdf = include_bytes!("../../../tests/fixtures/check.pdf");
        let report = import_receipt(&state, user_id, pdf).await.unwrap();
        let expense_id = report.expense_id.unwrap();

        expense_service::delete_expense(&state, user_id, expense_id)
            .await
            .unwrap();
        assert_eq!(list_pending_receipts(&state, user_id).await.unwrap().len(), 1);

        let report = expense_service::import_csv(&state, user_id, MATCH_CSV)
            .await
            .unwrap();
        assert_eq!(report.receipts_attached, 1);
        assert!(list_pending_receipts(&state, user_id)
            .await
            .unwrap()
            .is_empty());
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
