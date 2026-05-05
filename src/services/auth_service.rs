use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use rand_core::OsRng;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::models::User;
use crate::state::AppState;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
}

pub fn hash_password(password: &str) -> AppResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(hash.to_string())
}

pub fn verify_password(password: &str, hash: &str) -> AppResult<bool> {
    let parsed_hash = PasswordHash::new(hash)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

pub fn generate_token(user_id: &str, secret: &str, expires_in: i64) -> AppResult<String> {
    let exp = chrono::Utc::now().timestamp() as usize + expires_in as usize;
    let claims = Claims {
        sub: user_id.to_string(),
        exp,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(Into::into)
}

pub fn validate_token(token: &str, secret: &str) -> AppResult<Claims> {
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )?;
    Ok(token_data.claims)
}

pub async fn register(
    state: &AppState,
    email: String,
    password: String,
    jwt_secret: &str,
    jwt_expires_in: i64,
) -> AppResult<(String, User)> {
    {
        let users = state.users.read().await;
        if users.values().any(|u| u.email == email) {
            return Err(AppError::Conflict("email already registered".into()));
        }
    }

    let password_hash = hash_password(&password)?;
    let user = User::new(email, password_hash);
    let token = generate_token(&user.id.to_string(), jwt_secret, jwt_expires_in)?;

    state.users.write().await.insert(user.id, user.clone());

    Ok((token, user))
}

pub async fn login(
    state: &AppState,
    email: &str,
    password: &str,
    jwt_secret: &str,
    jwt_expires_in: i64,
) -> AppResult<(String, User)> {
    let (user_id, _password_hash) = {
        let users = state.users.read().await;
        let user = users
            .values()
            .find(|u| u.email == email)
            .ok_or(AppError::Unauthorized)?;

        if !verify_password(password, &user.password_hash)? {
            return Err(AppError::Unauthorized);
        }
        (user.id, user.password_hash.clone())
    };

    let token = generate_token(&user_id.to_string(), jwt_secret, jwt_expires_in)?;

    let users = state.users.read().await;
    let user = users.get(&user_id).cloned().ok_or(AppError::Unauthorized)?;
    Ok((token, user))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_and_verify_password() {
        let hash = hash_password("password123").unwrap();
        assert!(verify_password("password123", &hash).unwrap());
        assert!(!verify_password("wrong", &hash).unwrap());
    }

    #[test]
    fn test_generate_and_validate_token() {
        let secret = "test_secret";
        let token = generate_token("user-123", secret, 3600).unwrap();
        let claims = validate_token(&token, secret).unwrap();
        assert_eq!(claims.sub, "user-123");
    }

    #[test]
    fn test_invalid_token() {
        let claims = validate_token("invalid.token.here", "secret");
        assert!(claims.is_err());
    }
}
