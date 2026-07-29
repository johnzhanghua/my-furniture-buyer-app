use actix_cors::Cors;
use actix_web::middleware::Logger;
use actix_web::{web, App, HttpServer};

use furniture_buyer_api::config::Config;
use furniture_buyer_api::state::AppState;
use furniture_buyer_api::{db, external_api, routes};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

    let config = Config::from_env();

    let pool = db::init_pool(&config.database_url)
        .await
        .map_err(|e| std::io::Error::other(format!("database setup failed: {e}")))?;

    db::seed_demo_user(&pool)
        .await
        .map_err(|e| std::io::Error::other(format!("seeding failed: {e}")))?;

    let bind_addr = (config.host.clone(), config.port);
    let allowed_origins = config.cors_allowed_origins.clone();

    let external_api = external_api::ExternalApiClient::new(config.external_api.clone())
        .map_err(|e| std::io::Error::other(format!("HTTP client setup failed: {e}")))?;

    if external_api.is_configured() {
        log::info!(
            "upstream API configured at {} as user {}",
            config.external_api.base_url,
            config.external_api.user_id
        );
    } else {
        log::warn!("upstream API not configured; set EXTERNAL_API_BASE_URL and EXTERNAL_API_KEY");
    }

    let agent = furniture_buyer_api::agent::Agent::new(config.agent.clone())
        .map_err(|e| std::io::Error::other(format!("assistant setup failed: {e}")))?;

    if agent.is_configured() {
        log::info!(
            "assistant enabled: Azure OpenAI deployment {} at {}",
            config.agent.deployment,
            config.agent.endpoint
        );
    } else {
        log::warn!("assistant disabled; set AZURE_OPENAI_API_KEY to enable /api/assistant/ask");
    }

    let state = web::Data::new(AppState {
        pool,
        config: config.clone(),
        external_api,
        agent,
    });

    log::info!("API listening on http://{}:{}", bind_addr.0, bind_addr.1);

    HttpServer::new(move || {
        let mut cors = Cors::default()
            .allow_any_header()
            .allow_any_method()
            .max_age(3600);
        for origin in &allowed_origins {
            cors = cors.allowed_origin(origin);
        }

        App::new()
            .app_data(state.clone())
            .app_data(web::JsonConfig::default().limit(256 * 1024))
            .wrap(Logger::default())
            .wrap(cors)
            .configure(routes::configure)
    })
    .bind(bind_addr)?
    .run()
    .await
}
