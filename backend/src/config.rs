use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub host: String,
    pub port: u16,
    pub jwt_secret: String,
    pub jwt_ttl_hours: i64,
    pub cors_allowed_origins: Vec<String>,
    pub external_api: ExternalApiConfig,
    pub agent: AgentConfig,
}

/// Azure OpenAI settings for the in-app assistant.
#[derive(Clone)]
pub struct AgentConfig {
    /// Resource endpoint, e.g. `https://australiaeast.api.cognitive.microsoft.com/`.
    pub endpoint: String,
    pub api_version: String,
    /// Deployment name — this, not a `model` field, selects the model.
    pub deployment: String,
    pub api_key: String,
    /// Covers reasoning *and* the visible answer. `max_tokens` is rejected with
    /// a 400 on the gpt-5 reasoning family, so this is the only budget knob.
    pub max_completion_tokens: u32,
    /// `minimal` | `low` | `medium` | `high`. Tune for latency vs depth.
    pub reasoning_effort: String,
    pub timeout_secs: u64,
}

// Hand-written so the API key cannot reach a log line via `{:?}` on Config.
impl std::fmt::Debug for AgentConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentConfig")
            .field("endpoint", &self.endpoint)
            .field("api_version", &self.api_version)
            .field("deployment", &self.deployment)
            .field(
                "api_key",
                &if self.api_key.is_empty() {
                    "<unset>"
                } else {
                    "<redacted>"
                },
            )
            .field("max_completion_tokens", &self.max_completion_tokens)
            .field("reasoning_effort", &self.reasoning_effort)
            .field("timeout_secs", &self.timeout_secs)
            .finish()
    }
}

/// Credentials and connection details for the hackathon's upstream API.
///
/// Auth is a single `X-Api-Key` header. The participant `user_id` is *not* a
/// header — it is a path segment or body field depending on the endpoint, so
/// it lives here and is passed explicitly by each call in `external_api.rs`.
#[derive(Clone)]
pub struct ExternalApiConfig {
    pub base_url: String,
    pub user_id: String,
    pub api_key: String,
    pub key_header: String,
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
            external_api: ExternalApiConfig {
                // Trailing slashes are trimmed so paths can be joined as
                // `{base}/{path}` without producing a double slash.
                base_url: var_or(
                    "EXTERNAL_API_BASE_URL",
                    "https://day1.training.cognitivo.com.au",
                )
                .trim_end_matches('/')
                .to_string(),
                user_id: var_or("EXTERNAL_API_USER_ID", ""),
                api_key: var_or("EXTERNAL_API_KEY", ""),
                key_header: var_or("EXTERNAL_API_KEY_HEADER", "X-Api-Key"),
                timeout_secs: parse_or("EXTERNAL_API_TIMEOUT_SECS", 15),
            },
            agent: AgentConfig {
                endpoint: var_or("AZURE_OPENAI_ENDPOINT", "")
                    .trim_end_matches('/')
                    .to_string(),
                api_version: var_or("AZURE_OPENAI_API_VERSION", "2024-12-01-preview"),
                deployment: var_or("AZURE_OPENAI_DEPLOYMENT", "gpt-5-mini"),
                api_key: var_or("AZURE_OPENAI_API_KEY", ""),
                max_completion_tokens: parse_or("AGENT_MAX_COMPLETION_TOKENS", 16000),
                reasoning_effort: var_or("AGENT_REASONING_EFFORT", "low"),
                timeout_secs: parse_or("AGENT_TIMEOUT_SECS", 120),
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
