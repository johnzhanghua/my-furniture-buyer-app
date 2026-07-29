use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub host: String,
    pub port: u16,
    pub jwt_secret: String,
    pub jwt_ttl_hours: i64,
    pub cors_allowed_origins: Vec<String>,
    pub default_budget_cents: i64,
    pub external_api: ExternalApiConfig,
}

/// Credentials and connection details for the hackathon's upstream API.
///
/// The header names are configurable because the lab guide is behind a login
/// and the auth scheme has not been confirmed yet — if upstream expects
/// `Authorization: Bearer …` instead, that is an `.env` change, not a code
/// change. See `external_api.rs`.
#[derive(Clone)]
pub struct ExternalApiConfig {
    pub base_url: String,
    pub user_id: String,
    pub api_key: String,
    pub key_header: String,
    pub user_header: String,
    pub timeout_secs: u64,
}

impl ExternalApiConfig {
    /// True once a base URL and key are present; handlers should treat a
    /// half-configured upstream as "not available" rather than erroring late.
    pub fn is_configured(&self) -> bool {
        !self.base_url.is_empty() && !self.api_key.is_empty()
    }
}

// Hand-written so the API key cannot reach a log line via `{:?}` on Config.
impl std::fmt::Debug for ExternalApiConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExternalApiConfig")
            .field("base_url", &self.base_url)
            .field("user_id", &self.user_id)
            .field("api_key", &"<redacted>")
            .field("key_header", &self.key_header)
            .field("user_header", &self.user_header)
            .field("timeout_secs", &self.timeout_secs)
            .finish()
    }
}

impl Config {
    /// Reads configuration from the environment, falling back to dev defaults.
    pub fn from_env() -> Self {
        Config {
            database_url: var_or("DATABASE_URL", "sqlite://furniture.db"),
            host: var_or("HOST", "127.0.0.1"),
            port: parse_or("PORT", 8080),
            jwt_secret: var_or("JWT_SECRET", "dev-only-change-me"),
            jwt_ttl_hours: parse_or("JWT_TTL_HOURS", 24),
            cors_allowed_origins: var_or(
                "CORS_ALLOWED_ORIGINS",
                "http://localhost:5173,http://127.0.0.1:5173",
            )
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
            default_budget_cents: parse_or("DEFAULT_BUDGET_CENTS", 500_000),
            external_api: ExternalApiConfig {
                // Trailing slashes are trimmed so paths can be joined as
                // `{base}/{path}` without producing a double slash.
                base_url: var_or("EXTERNAL_API_BASE_URL", "")
                    .trim_end_matches('/')
                    .to_string(),
                user_id: var_or("EXTERNAL_API_USER_ID", ""),
                api_key: var_or("EXTERNAL_API_KEY", ""),
                key_header: var_or("EXTERNAL_API_KEY_HEADER", "X-API-Key"),
                user_header: var_or("EXTERNAL_API_USER_HEADER", "X-User-Id"),
                timeout_secs: parse_or("EXTERNAL_API_TIMEOUT_SECS", 10),
            },
        }
    }
}

fn var_or(key: &str, fallback: &str) -> String {
    env::var(key).unwrap_or_else(|_| fallback.to_string())
}

fn parse_or<T: std::str::FromStr>(key: &str, fallback: T) -> T {
    env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(fallback)
}
