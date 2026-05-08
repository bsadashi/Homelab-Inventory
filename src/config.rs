//! Runtime configuration. Everything is sourced from the environment so the
//! same binary can be promoted across local / container / Kubernetes without
//! rebuilding.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// Resolved server configuration.
#[derive(Debug, Clone)]
pub struct Config {
    pub bind: SocketAddr,
    pub database_url: String,
    pub data_dir: PathBuf,
    pub static_dir: Option<PathBuf>,
    /// Wrapped in `Arc` so cloning the config (cheaply, on every
    /// request via `AppState`) doesn't copy the token bytes.
    pub auth_token: Option<Arc<String>>,
    pub seed_on_empty: bool,
    /// When true, /api/auth/signup accepts requests even after the
    /// first user has been created. Useful for trusted-network
    /// homelabs where everyone can self-register; off by default.
    pub open_signup: bool,
    pub request_timeout: Duration,
    pub max_body_bytes: usize,
    pub log_format: LogFormat,
    pub allow_origin: AllowOrigin,
    pub trust_forwarded_headers: bool,
    /// Shared secret for HMAC verification on /api/events. When
    /// set, ingest requires both `X-Racklog-Signature` and
    /// `X-Racklog-Timestamp` headers — see `routes/events.rs`.
    /// When unset, /api/events accepts any authed request, which
    /// is fine for a homelab where API keys are tightly scoped
    /// but worth tightening when sensors live outside the trust
    /// boundary.
    pub events_hmac_secret: Option<String>,
    /// Maximum live sessions per user. Older sessions get evicted
    /// FIFO when a new login pushes past the cap.
    pub max_sessions_per_user: i64,
    /// Comma-separated CIDR ranges allowed to set X-Forwarded-User
    /// / X-Forwarded-Groups. Only consulted when
    /// `trust_forwarded_headers` is true. Empty means "trust any
    /// peer that can reach the bind socket" — fine on a loopback
    /// bind behind a single proxy, dangerous otherwise.
    pub trusted_proxy_cidrs: Vec<ipnet::IpNet>,
}

#[derive(Debug, Clone, Copy)]
pub enum LogFormat {
    Pretty,
    Json,
}

#[derive(Debug, Clone)]
pub enum AllowOrigin {
    /// Same-origin only — the default and recommended setting.
    SameOrigin,
    /// Explicit list of trusted origins.
    List(Vec<String>),
}

impl Config {
    /// Test-only constructor with in-memory-friendly defaults. Skips
    /// dotenv and never reads the process environment so tests stay
    /// hermetic and parallel-safe.
    pub fn for_test(database_url: impl Into<String>, auth_token: Option<String>) -> Self {
        Self {
            bind: "127.0.0.1:0".parse().unwrap(),
            database_url: database_url.into(),
            data_dir: PathBuf::from("./data"),
            static_dir: None,
            auth_token: auth_token.map(Arc::new),
            seed_on_empty: true,
            open_signup: false,
            request_timeout: Duration::from_secs(30),
            max_body_bytes: 2 * 1024 * 1024,
            log_format: LogFormat::Pretty,
            allow_origin: AllowOrigin::SameOrigin,
            trust_forwarded_headers: false,
            events_hmac_secret: None,
            max_sessions_per_user: 10,
            trusted_proxy_cidrs: Vec::new(),
        }
    }

    /// Build the runtime config from process environment variables. Missing
    /// values fall back to safe local-first defaults.
    pub fn from_env() -> Result<Self, ConfigError> {
        let _ = dotenvy::dotenv();

        let bind: SocketAddr = std::env::var("RACKLOG_BIND")
            .unwrap_or_else(|_| "0.0.0.0:8080".to_string())
            .parse()
            .map_err(|e| ConfigError::Invalid("RACKLOG_BIND", format!("{e}")))?;

        let data_dir = PathBuf::from(
            std::env::var("RACKLOG_DATA_DIR").unwrap_or_else(|_| "./data".to_string()),
        );

        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            let mut p = data_dir.clone();
            p.push("racklog.db");
            // sqlx wants `sqlite://` URLs; `?mode=rwc` autocreates the file.
            format!("sqlite://{}?mode=rwc", p.display())
        });

        let static_dir = std::env::var("RACKLOG_STATIC_DIR")
            .ok()
            .filter(|v| !v.is_empty())
            .map(PathBuf::from);

        let auth_token = std::env::var("RACKLOG_AUTH_TOKEN")
            .ok()
            .filter(|v| !v.is_empty())
            .map(Arc::new);

        let seed_on_empty = parse_bool("RACKLOG_SEED_ON_EMPTY", true);
        let open_signup = parse_bool("RACKLOG_OPEN_SIGNUP", false);

        let request_timeout = Duration::from_secs(parse_u64("RACKLOG_REQUEST_TIMEOUT_SECS", 30));
        let max_body_bytes = parse_usize("RACKLOG_MAX_BODY_BYTES", 2 * 1024 * 1024);

        let log_format = match std::env::var("RACKLOG_LOG_FORMAT").as_deref() {
            Ok("json") => LogFormat::Json,
            _ => LogFormat::Pretty,
        };

        let allow_origin = match std::env::var("RACKLOG_ALLOW_ORIGIN").as_deref() {
            Ok(v) if !v.is_empty() => {
                AllowOrigin::List(v.split(',').map(|s| s.trim().to_string()).collect())
            }
            _ => AllowOrigin::SameOrigin,
        };

        let trust_forwarded_headers = parse_bool("RACKLOG_TRUST_FORWARDED_HEADERS", false);
        let events_hmac_secret = std::env::var("RACKLOG_EVENTS_HMAC_SECRET")
            .ok()
            .filter(|s| !s.is_empty());
        let max_sessions_per_user: i64 = std::env::var("RACKLOG_MAX_SESSIONS_PER_USER")
            .ok()
            .and_then(|s| s.parse().ok())
            .filter(|n: &i64| *n >= 1)
            .unwrap_or(10);

        let trusted_proxy_cidrs = match std::env::var("RACKLOG_TRUSTED_PROXY_CIDR") {
            Ok(v) if !v.is_empty() => {
                let mut out = Vec::new();
                for part in v.split(',') {
                    let p = part.trim();
                    if p.is_empty() {
                        continue;
                    }
                    let net: ipnet::IpNet = p.parse().map_err(|e: ipnet::AddrParseError| {
                        ConfigError::Invalid("RACKLOG_TRUSTED_PROXY_CIDR", format!("{p}: {e}"))
                    })?;
                    out.push(net);
                }
                out
            }
            _ => Vec::new(),
        };

        Ok(Self {
            bind,
            database_url,
            data_dir,
            static_dir,
            auth_token,
            seed_on_empty,
            open_signup,
            request_timeout,
            max_body_bytes,
            log_format,
            allow_origin,
            trust_forwarded_headers,
            events_hmac_secret,
            max_sessions_per_user,
            trusted_proxy_cidrs,
        })
    }
}

fn parse_bool(name: &str, default: bool) -> bool {
    match std::env::var(name).ok().as_deref() {
        Some("1") | Some("true") | Some("TRUE") | Some("yes") => true,
        Some("0") | Some("false") | Some("FALSE") | Some("no") => false,
        _ => default,
    }
}

fn parse_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn parse_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("invalid value for {0}: {1}")]
    Invalid(&'static str, String),
}
