use chrono::NaiveDate;
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Category {
    Food,
    House,
    Transport,
    Deposits,
    Gifts,
    Entertainment,
    Toiletry,
    Communications,
    Bills,
    Salary,
}

impl std::str::FromStr for Category {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Food" => Ok(Category::Food),
            "House" => Ok(Category::House),
            "Transport" => Ok(Category::Transport),
            "Deposits" => Ok(Category::Deposits),
            "Gifts" => Ok(Category::Gifts),
            "Entertainment" => Ok(Category::Entertainment),
            "Toiletry" => Ok(Category::Toiletry),
            "Communications" => Ok(Category::Communications),
            "Bills" => Ok(Category::Bills),
            "Salary" => Ok(Category::Salary),
            _ => Err(format!("unknown category: {}", s)),
        }
    }
}

impl std::fmt::Display for Category {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Category::Food => write!(f, "Food"),
            Category::House => write!(f, "House"),
            Category::Transport => write!(f, "Transport"),
            Category::Deposits => write!(f, "Deposits"),
            Category::Gifts => write!(f, "Gifts"),
            Category::Entertainment => write!(f, "Entertainment"),
            Category::Toiletry => write!(f, "Toiletry"),
            Category::Communications => write!(f, "Communications"),
            Category::Bills => write!(f, "Bills"),
            Category::Salary => write!(f, "Salary"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Expense {
    pub id: Uuid,
    pub user_id: Uuid,
    pub date: NaiveDate,
    pub account: Account,
    pub category: Category,
    pub amount: f64,
    pub currency: String,
    pub description: String,
}

impl Expense {
    pub fn is_income(&self) -> bool {
        self.amount > 0.0
    }

    pub fn is_expense(&self) -> bool {
        self.amount < 0.0
    }
}
