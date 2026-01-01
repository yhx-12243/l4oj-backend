use axum::{Router, routing::post};
use http::StatusCode;

use crate::libs::response::JkmxJsonResponse;

mod tag;

async fn query_problem_set() -> JkmxJsonResponse {
    let res = r#"{"count":0,"result":[],"permissions":{"createProblem":true,"manageTags":true,"filterByOwner":true,"filterNonpublic":true}}"#;
    JkmxJsonResponse::Response(StatusCode::OK, res.into())
}

pub fn router() -> Router {
    Router::new()
        .route("/queryProblemSet", post(query_problem_set))
        .merge(tag::router())
}
