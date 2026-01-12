use axum::Router;
use http::{Response, header};
use tower_http::cors::CorsLayer;

use crate::libs::constants::APPLICATION_JSON_UTF_8;

mod auth;
mod discussion;
pub mod fs;
mod group;
mod homepage;
mod judge_client;
mod problem;
mod submission;
mod user;

pub fn all() -> Router {
    let cors = CorsLayer::very_permissive().allow_private_network(true);

    let mut parts = Response::new(()).into_parts().0;
    parts.headers.insert(header::CONTENT_TYPE, APPLICATION_JSON_UTF_8);
    let header = Box::leak(Box::new(parts));

    Router::new()
        .nest("/auth", auth::router(header))
        .nest("/discussion", discussion::router(header))
        .nest("/group", group::router(header))
        .nest("/homepage", homepage::router(header))
        .nest("/judgeClient", judge_client::router(header))
        .nest("/problem", problem::router(header))
        .nest("/submission", submission::router(header))
        .nest("/user", user::router(header))
        .layer(cors)
}
