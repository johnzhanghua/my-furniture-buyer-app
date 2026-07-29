pub mod assistant;
pub mod auth;
pub mod me;
pub mod orders;
pub mod products;
pub mod upstream;

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
                    .route("/balance", web::get().to(me::balance)),
            )
            .route("/categories", web::get().to(products::categories))
            .service(web::scope("/assistant").route("/ask", web::post().to(assistant::ask)))
            .service(
                web::scope("/products")
                    .route("", web::get().to(products::list))
                    .route("/{id}", web::get().to(products::detail))
                    .route("/{id}/image", web::get().to(products::image)),
            )
            .service(
                web::scope("/orders")
                    .route("", web::post().to(orders::create))
                    .route("", web::get().to(orders::list)),
            )
            .service(web::scope("/upstream").route("/status", web::get().to(upstream::status))),
    );
}

async fn health() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({ "status": "ok" }))
}
