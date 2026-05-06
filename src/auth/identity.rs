//! Authentication identity threaded through every authenticated
//! request. Lives in axum request extensions so handlers can pull it
//! out via `axum::Extension<AuthIdentity>`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    Operator,
    #[default]
    Viewer,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::Operator => "operator",
            Role::Viewer => "viewer",
        }
    }

    /// Allowed to perform any mutation (POST/PUT/DELETE on inventory).
    pub fn can_write(self) -> bool {
        matches!(self, Role::Admin | Role::Operator)
    }

    /// Allowed to reach `/api/admin/*` and manage users.
    pub fn can_admin(self) -> bool {
        matches!(self, Role::Admin)
    }

    /// Numeric strength so we can compare "at least this role".
    fn level(self) -> u8 {
        match self {
            Role::Viewer => 1,
            Role::Operator => 2,
            Role::Admin => 3,
        }
    }

    pub fn at_least(self, other: Role) -> bool {
        self.level() >= other.level()
    }
}

impl std::str::FromStr for Role {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "admin" => Ok(Role::Admin),
            "operator" => Ok(Role::Operator),
            "viewer" => Ok(Role::Viewer),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthSource {
    Session,
    ApiKey,
    Bootstrap,
    TrustedHeader,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuthIdentity {
    pub user_id: String,
    pub username: String,
    pub role: Role,
    pub source: AuthSource,
}

impl AuthIdentity {
    pub fn bootstrap_admin() -> Self {
        Self {
            user_id: "bootstrap".into(),
            username: "bootstrap".into(),
            role: Role::Admin,
            source: AuthSource::Bootstrap,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_ordering_is_admin_strongest() {
        assert!(Role::Admin.at_least(Role::Operator));
        assert!(Role::Admin.at_least(Role::Viewer));
        assert!(Role::Operator.at_least(Role::Viewer));
        assert!(!Role::Viewer.at_least(Role::Operator));
        assert!(!Role::Operator.at_least(Role::Admin));
    }

    #[test]
    fn permission_helpers_are_consistent() {
        assert!(Role::Admin.can_admin());
        assert!(Role::Admin.can_write());
        assert!(!Role::Operator.can_admin());
        assert!(Role::Operator.can_write());
        assert!(!Role::Viewer.can_write());
    }

    #[test]
    fn from_str_round_trips() {
        for r in [Role::Admin, Role::Operator, Role::Viewer] {
            assert_eq!(r.as_str().parse::<Role>().unwrap(), r);
        }
        assert!("nonsense".parse::<Role>().is_err());
    }
}
