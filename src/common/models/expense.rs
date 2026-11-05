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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransactionKind {
    Expense,
    Income,
}

impl std::fmt::Display for TransactionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransactionKind::Expense => write!(f, "expense"),
            TransactionKind::Income => write!(f, "income"),
        }
    }
}

impl std::str::FromStr for TransactionKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "expense" => Ok(TransactionKind::Expense),
            "income" => Ok(TransactionKind::Income),
            _ => Err(format!("unknown kind: {}", s)),
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
    pub kind: TransactionKind,
    pub currency: String,
    pub amount_usd: Option<Decimal>,
    pub description: String,
}
