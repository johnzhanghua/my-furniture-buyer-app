mod auth;
mod config;
mod db;
mod error;
mod models;
mod routes;
mod state;

use actix_cors::Cors;
use actix_web::middleware::Logger;
use actix_web::{web, App, HttpServer};

use crate::config::Config;
use crate::state::AppState;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

    let config = Config::from_env();

    let pool = db::init_pool(&config.database_url)
        .await
        .map_err(|e| std::io::Error::other(format!("database setup failed: {e}")))?;

    db::seed_demo_user(&pool, config.default_budget_cents)
        .await
        .map_err(|e| std::io::Error::other(format!("seeding failed: {e}")))?;

    let bind_addr = (config.host.clone(), config.port);
    let allowed_origins = config.cors_allowed_origins.clone();

    let state = web::Data::new(AppState {
        pool,
        config: config.clone(),
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
