use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::Serialize;
use uuid::Uuid;

use super::expense::Account;

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedItem {
    pub code: String,
    pub name: String,
    pub unit_price: Decimal,
    pub quantity: Decimal,
    pub discount: Decimal,
    pub total: Decimal,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedReceipt {
    pub date: NaiveDate,
    pub account: Account,
    pub total: Decimal,
    pub fiscal_id: String,
    pub items: Vec<ParsedItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptStatus {
    Attached,
    Pending,
    Duplicate,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReceiptReport {
    pub status: ReceiptStatus,
    pub date: NaiveDate,
    pub total: Decimal,
    pub currency: String,
    pub item_count: usize,
    pub expense_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct ReceiptItem {
    pub receipt_id: Uuid,
    pub position: i64,
    pub code: String,
    pub name: String,
    pub unit_price: Decimal,
    pub quantity: Decimal,
    pub discount: Decimal,
    pub total: Decimal,
}

#[derive(Debug, Serialize)]
pub struct Receipt {
    pub id: Uuid,
    pub user_id: Uuid,
    pub expense_id: Option<Uuid>,
    pub date: NaiveDate,
    pub total: Decimal,
    pub currency: String,
    pub account: Account,
    pub fiscal_id: String,
    pub items: Vec<ReceiptItem>,
}
