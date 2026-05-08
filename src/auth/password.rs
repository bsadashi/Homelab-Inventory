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
///
/// The cost parameters are reduced under `cfg(test)` so the test suite
/// — which signs up users in nearly every test — runs in seconds
/// instead of tens of seconds. Production cost stays at the library
/// default.
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
    argon2_engine()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| PasswordError::Invalid(e.to_string()))
}

#[cfg(not(test))]
fn argon2_engine() -> Argon2<'static> {
    Argon2::default()
}

/// Test-only fast-path: minimum legal cost. Argon2 still computes
/// real work but ~50× faster than the production defaults — the
/// difference between a 12 s test suite and a 3 s one. Production
/// hashing is unaffected.
#[cfg(test)]
fn argon2_engine() -> Argon2<'static> {
    use argon2::{Algorithm, Params, Version};
    let params = Params::new(
        Params::MIN_M_COST,
        Params::MIN_T_COST,
        Params::MIN_P_COST,
        None,
    )
    .expect("argon2 params");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
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
    let parsed =
        PasswordHash::new(stored_hash).map_err(|e| PasswordError::Invalid(e.to_string()))?;
    // Verify uses the params encoded in the PHC string itself, so
    // Argon2::default() works for any cost we ever produced. The
    // test-only `argon2_engine()` above only matters for *new* hashes.
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

/// Run an Argon2 verify against a fixed never-matching hash so that
/// callers on the "no user found" code path spend the same wall-time
/// as the "user found, wrong password" path. Used by the login
/// handler — without this, the difference between
/// `password::verify(_, "")` (immediate) and a real verify (~25ms
/// production) is a clean username-existence oracle.
///
/// The dummy hash is computed once at first call using the same
/// `argon2_engine()` we use for real password hashing, so cost
/// parameters match between the two paths under both production
/// and `#[cfg(test)]`.
pub fn verify_dummy(password: &str) {
    use std::sync::OnceLock;
    static DUMMY: OnceLock<String> = OnceLock::new();
    let h = DUMMY.get_or_init(|| {
        // The actual content is irrelevant — verify() never matches
        // the password against this, and we discard the result. We
        // call hash() rather than embedding a literal PHC string so
        // the cost parameters always track what argon2_engine()
        // produces.
        hash("__racklog_dummy_password__")
            .expect("dummy hash must hash cleanly")
    });
    let _ = verify(password, h);
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
