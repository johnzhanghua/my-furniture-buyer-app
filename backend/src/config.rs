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
