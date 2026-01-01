use axum::Router;
use tower_http::cors::CorsLayer;

mod auth;
mod homepage;
mod user;

pub fn all() -> Router {
    let cors = CorsLayer::very_permissive().allow_private_network(true);

    Router::new()
        .nest("/auth", auth::router())
        .nest("/homepage", homepage::router())
        .nest("/user", user::router())
        .layer(cors)
}
