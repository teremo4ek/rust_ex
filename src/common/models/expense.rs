use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Account {
    Cash,
    PaymentCard,
}

impl std::fmt::Display for Account {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Account::Cash => write!(f, "Cash"),
            Account::PaymentCard => write!(f, "Payment card"),
        }
    }
}

impl std::str::FromStr for Account {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Cash" => Ok(Account::Cash),
            "Payment card" => Ok(Account::PaymentCard),
            _ => Err(format!("unknown account: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Expense {
    pub id: Uuid,
    pub user_id: Uuid,
    pub date: NaiveDate,
    pub account: Account,
    pub category: String,
    pub amount: Decimal,
    pub currency: String,
    pub description: String,
}

impl Expense {
    pub fn is_income(&self) -> bool {
        self.amount > Decimal::ZERO
    }

    pub fn is_expense(&self) -> bool {
        self.amount < Decimal::ZERO
    }
}
