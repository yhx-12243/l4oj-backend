use axum::Router;
use tower_http::cors::CorsLayer;

pub mod auth;

pub fn all() -> Router {
    let cors = CorsLayer::very_permissive().allow_private_network(true);

    Router::new()
        .nest("/auth", auth::router())
        .layer(cors)
}
