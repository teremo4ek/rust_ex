use chrono::NaiveDate;
use serde::Deserialize;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy)]
pub enum Currency {
    Usd,
    Eur,
}

impl Currency {
    fn as_str(&self) -> &'static str {
        match self {
            Currency::Usd => "USD",
            Currency::Eur => "EUR",
        }
    }
}

#[derive(Deserialize)]
struct RateResponse {
    #[serde(rename = "Cur_OfficialRate")]
    rate: f64,
}

// check api https://www.nbrb.by/apihelp/exrates
pub async fn get_rate(currency: Currency, date: NaiveDate) -> AppResult<f64> {
    let url = format!(
        "https://api.nbrb.by/exrates/rates/{}?parammode=2&ondate={}",
        currency.as_str(),
        date
    );

    let response = reqwest::get(&url)
        .await
        .map_err(|e| AppError::ExternalApi(e.to_string()))?;

    if !response.status().is_success() {
        return Err(AppError::ExternalApi(format!(
            "NBRB API returned status {} for {}",
            response.status(),
            url
        )));
    }

    let rate: RateResponse = response
        .json()
        .await
        .map_err(|e| AppError::ExternalApi(e.to_string()))?;

    Ok(rate.rate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn usd_rate_on_fixed_date() {
        let rate = get_rate(Currency::Usd, NaiveDate::from_ymd_opt(2023, 1, 10).unwrap())
            .await
            .unwrap();
        assert!((rate - 2.7101).abs() < 1e-4);
    }

    #[tokio::test]
    async fn eur_rate_on_fixed_date() {
        let rate = get_rate(Currency::Eur, NaiveDate::from_ymd_opt(2023, 8, 21).unwrap())
            .await
            .unwrap();
        assert!((rate - 3.4009).abs() < 1e-4);
    }

    #[tokio::test]
    async fn future_date_returns_error() {
        let result = get_rate(Currency::Usd, NaiveDate::from_ymd_opt(2030, 1, 1).unwrap()).await;
        assert!(result.is_err());
    }
}
