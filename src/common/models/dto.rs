use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use super::{PublicUser, Expense, Receipt, ReceiptStatus};

#[derive(Debug, Deserialize, Validate)]
pub struct RegisterRequest {
    #[validate(email(message = "invalid email"))]
    pub email: String,
    #[validate(length(min = 8, message = "password must be at least 8 characters"))]
    pub password: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct LoginRequest {
    #[validate(email(message = "invalid email"))]
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: PublicUser,
}

#[derive(Debug, Deserialize)]
pub struct ExpensesQuery {
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    pub category: Option<String>,
    pub account: Option<String>,
    pub page: Option<u32>,
    pub per_page: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct UploadResponse {
    pub imported: usize,
    pub duplicates: usize,
    pub receipts_attached: usize,
    pub converted: usize,
    pub skipped: usize,
}

#[derive(Debug, Serialize)]
pub struct ReceiptUploadResponse {
    pub status: ReceiptStatus,
    pub date: NaiveDate,
    pub total: Decimal,
    pub currency: String,
    pub item_count: usize,
    pub expense_id: Option<Uuid>,
    pub converted: usize,
    pub skipped: usize,
}

#[derive(Debug, Serialize)]
pub struct PendingReceiptsResponse {
    pub receipts: Vec<Receipt>,
}

#[derive(Debug, Deserialize)]
pub struct ReceiptUrlRequest {
    pub url: String,
}

#[derive(Debug, Serialize)]
pub struct ExpensesResponse {
    pub expenses: Vec<Expense>,
    pub total: usize,
    pub page: u32,
    pub per_page: u32,
}

#[derive(Debug, Deserialize)]
pub struct AnalyticsQuery {
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    pub group_by: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SummaryResponse {
    pub total_income: Decimal,
    pub total_expense: Decimal,
    pub net: Decimal,
}

#[derive(Debug, Serialize)]
pub struct CategoryBreakdown {
    pub category: String,
    pub amount: Decimal,
    pub percentage: Decimal,
}

#[derive(Debug, Serialize)]
pub struct AccountBreakdown {
    pub account: String,
    pub income: Decimal,
    pub expense: Decimal,
}

#[derive(Debug, Serialize)]
pub struct TimelinePoint {
    pub period: String,
    pub income: Decimal,
    pub expense: Decimal,
}
