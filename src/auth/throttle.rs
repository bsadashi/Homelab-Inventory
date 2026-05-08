//! Per-username token-bucket throttle for /api/auth/login.
//!
//! Pen-test finding (MEDIUM severity): the login endpoint had no
//! brute-force protection — 30 wrong-password attempts in <2s all
//! returned 401 with no slowdown. Argon2id at default cost provides
//! a natural ~25ms-per-attempt floor, but that's not enough on its
//! own against a long dictionary attack on a known-valid username.
//!
//! This module maintains a process-wide map of username → token
//! bucket. Each login attempt (success or failure) consumes one
//! token; when the bucket runs dry the handler returns 429 instead
//! of letting the attempt proceed. On a successful login the bucket
//! is refilled so the user isn't punished for one typo before
//! getting in.
//!
//! Single-replica is the assumed deployment shape (it matches the
//! rest of the SQLite-backed posture). Horizontal scaling needs an
//! out-of-process limiter — same caveat as the lookup token bucket.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

#[derive(Debug)]
struct Bucket {
    tokens: f64,
    capacity: f64,
    refill_per_sec: f64,
    last: Instant,
}

impl Bucket {
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last).as_secs_f64();
        self.last = now;
        self.tokens = (self.tokens + elapsed * self.refill_per_sec).min(self.capacity);
    }

    fn try_take(&mut self, n: f64) -> bool {
        self.refill();
        if self.tokens >= n {
            self.tokens -= n;
            true
        } else {
            false
        }
    }

    fn restore(&mut self) {
        self.tokens = self.capacity;
        self.last = Instant::now();
    }
}

fn config() -> (f64, f64) {
    let per_min = std::env::var("RACKLOG_LOGIN_ATTEMPTS_PER_MIN")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(5.0)
        .max(1.0);
    let burst = std::env::var("RACKLOG_LOGIN_BURST")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(10.0)
        .max(1.0);
    (per_min, burst)
}

fn bucket_for<'a>(map: &'a mut HashMap<String, Bucket>, key: &str) -> &'a mut Bucket {
    let (per_min, burst) = config();
    map.entry(key.to_string()).or_insert(Bucket {
        tokens: burst,
        capacity: burst,
        refill_per_sec: per_min / 60.0,
        last: Instant::now(),
    })
}

/// Charge a login attempt against `username`'s bucket. Returns true
/// when the attempt is allowed to proceed. Call this *before* any
/// password::verify so a bot can't spend Argon2 cost we already
/// declined to spend.
pub fn allow_attempt(username: &str) -> bool {
    let mut map = bucket_map().lock().expect("login throttle mutex");
    let b = bucket_for(&mut map, username);
    b.try_take(1.0)
}

/// Refill the bucket on successful login so a one-typo user isn't
/// punished. Also called from /api/auth/signup so the very first
/// login post-signup never trips the limiter.
pub fn note_success(username: &str) {
    let mut map = bucket_map().lock().expect("login throttle mutex");
    let b = bucket_for(&mut map, username);
    b.restore();
}

fn bucket_map() -> &'static Mutex<HashMap<String, Bucket>> {
    use std::sync::OnceLock;
    static MAP: OnceLock<Mutex<HashMap<String, Bucket>>> = OnceLock::new();
    MAP.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Test-only helper: drop all buckets so each test starts with a
/// fresh quota. Public so integration tests under tests/ can reach
/// it; not under #[cfg(test)] because the library-under-test is
/// always compiled in release mode for integration tests.
#[doc(hidden)]
pub fn reset_for_tests() {
    bucket_map().lock().unwrap().clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bucket_allows_burst_then_blocks() {
        reset_for_tests();
        // Burst of 10 by default; first 10 succeed, 11th fails.
        for _ in 0..10 {
            assert!(allow_attempt("brute_target"));
        }
        assert!(!allow_attempt("brute_target"));
    }

    #[test]
    fn buckets_are_per_username() {
        reset_for_tests();
        // Drain one user's bucket.
        for _ in 0..10 {
            allow_attempt("alice");
        }
        // A different user is unaffected.
        assert!(allow_attempt("bob"));
    }

    #[test]
    fn note_success_resets_bucket() {
        reset_for_tests();
        for _ in 0..10 {
            allow_attempt("carol");
        }
        assert!(!allow_attempt("carol"));
        note_success("carol");
        assert!(allow_attempt("carol"));
    }
}
