//! Argon2id password hashing.

use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use rand_core::OsRng;

#[derive(Debug, thiserror::Error)]
pub enum PasswordError {
    #[error("password is empty")]
    Empty,
    #[error("password too short (min {min})")]
    TooShort { min: usize },
    #[error("password too long (max {max})")]
    TooLong { max: usize },
    #[error("invalid hash: {0}")]
    Invalid(String),
}

pub const MIN_LEN: usize = 8;
pub const MAX_LEN: usize = 256;

/// Hash a fresh password with Argon2id (default OWASP-recommended params).
pub fn hash(password: &str) -> Result<String, PasswordError> {
    if password.is_empty() {
        return Err(PasswordError::Empty);
    }
    if password.len() < MIN_LEN {
        return Err(PasswordError::TooShort { min: MIN_LEN });
    }
    if password.len() > MAX_LEN {
        return Err(PasswordError::TooLong { max: MAX_LEN });
    }
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| PasswordError::Invalid(e.to_string()))
}

/// Verify a password against a stored Argon2 PHC string.
///
/// Returns `Ok(true)` on match, `Ok(false)` on a clean mismatch, and
/// `Err(...)` only when the stored hash is malformed.
pub fn verify(password: &str, stored_hash: &str) -> Result<bool, PasswordError> {
    if stored_hash.is_empty() {
        // SSO-provisioned users have no local password. Calling
        // verify() with their hash should be a clean false, not an
        // error — they authenticate via headers, not passwords.
        return Ok(false);
    }
    let parsed = PasswordHash::new(stored_hash)
        .map_err(|e| PasswordError::Invalid(e.to_string()))?;
    let argon2 = Argon2::default();
    Ok(argon2.verify_password(password.as_bytes(), &parsed).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let h = hash("correcthorse").unwrap();
        assert!(h.starts_with("$argon2"), "hash must be PHC-encoded");
        assert!(verify("correcthorse", &h).unwrap());
        assert!(!verify("wrong", &h).unwrap());
    }

    #[test]
    fn rejects_short_password() {
        assert!(matches!(hash("short"), Err(PasswordError::TooShort { .. })));
    }

    #[test]
    fn rejects_empty() {
        assert!(matches!(hash(""), Err(PasswordError::Empty)));
    }

    #[test]
    fn empty_stored_hash_is_clean_false() {
        assert!(!verify("anything", "").unwrap());
    }

    #[test]
    fn malformed_stored_hash_is_error() {
        assert!(verify("anything", "not-a-phc-string").is_err());
    }
}
