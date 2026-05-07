//! Shared pagination helpers for list endpoints.
//!
//! Most list endpoints share the same `?limit=…&offset=…` shape with
//! the same defaults and clamps. Duplicating the struct + clamp logic
//! in every route file is busywork — and a place where defaults can
//! drift between endpoints over time. One definition, one source of
//! truth.

use serde::Deserialize;

/// Largest page size any list endpoint will return regardless of
/// what the client asks for. Sized for the typical homelab catalog.
pub const MAX_LIMIT: i64 = 2000;

/// Default page size when the client doesn't ask for one.
pub const DEFAULT_LIMIT: i64 = 200;

#[derive(Debug, Deserialize, Default)]
pub struct PageQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

impl PageQuery {
    /// Resolved (limit, offset) with the canonical defaults + clamps.
    pub fn resolve(&self) -> (i64, i64) {
        let limit = self.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
        let offset = self.offset.unwrap_or(0).max(0);
        (limit, offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_when_none() {
        let p = PageQuery::default();
        assert_eq!(p.resolve(), (DEFAULT_LIMIT, 0));
    }

    #[test]
    fn clamps_high_limit() {
        let p = PageQuery {
            limit: Some(99_999),
            offset: None,
        };
        assert_eq!(p.resolve(), (MAX_LIMIT, 0));
    }

    #[test]
    fn clamps_negative_offset_to_zero() {
        let p = PageQuery {
            limit: None,
            offset: Some(-50),
        };
        assert_eq!(p.resolve(), (DEFAULT_LIMIT, 0));
    }

    #[test]
    fn clamps_low_limit_to_one() {
        let p = PageQuery {
            limit: Some(0),
            offset: None,
        };
        assert_eq!(p.resolve(), (1, 0));
    }
}
