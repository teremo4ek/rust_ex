use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::{PublicUser, Expense};

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
    pub total_income: f64,
    pub total_expense: f64,
    pub net: f64,
}

#[derive(Debug, Serialize)]
pub struct CategoryBreakdown {
    pub category: String,
    pub amount: f64,
    pub percentage: f64,
}

#[derive(Debug, Serialize)]
pub struct AccountBreakdown {
    pub account: String,
    pub income: f64,
    pub expense: f64,
}

#[derive(Debug, Serialize)]
pub struct TimelinePoint {
    pub period: String,
    pub income: f64,
    pub expense: f64,
}
