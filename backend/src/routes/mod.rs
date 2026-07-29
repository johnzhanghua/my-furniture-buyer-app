pub mod auth;
pub mod me;
pub mod orders;
pub mod products;

use actix_web::{web, HttpResponse};

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api")
            .route("/health", web::get().to(health))
            .service(
                web::scope("/auth")
                    .route("/register", web::post().to(auth::register))
                    .route("/login", web::post().to(auth::login)),
            )
            .service(
                web::scope("/me")
                    .route("", web::get().to(me::profile))
                    .route("/budget", web::get().to(me::budget)),
            )
            .service(
                web::scope("/products")
                    .route("", web::get().to(products::list))
                    .route("/{id}", web::get().to(products::detail)),
            )
            .service(
                web::scope("/orders")
                    .route("", web::post().to(orders::create))
                    .route("", web::get().to(orders::list))
                    .route("/{id}", web::get().to(orders::detail)),
            ),
    );
}

async fn health() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({ "status": "ok" }))
}
