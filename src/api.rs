use axum::Router;
use tower_http::cors::CorsLayer;

mod auth;
pub mod fs;
mod homepage;
mod problem;
mod user;

pub fn all() -> Router {
    let cors = CorsLayer::very_permissive().allow_private_network(true);

    Router::new()
        .nest("/auth", auth::router())
        .nest("/homepage", homepage::router())
        .nest("/problem", problem::router())
        .nest("/user", user::router())
        .layer(cors)
}
