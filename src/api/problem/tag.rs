use axum::{Json, Router, routing::post};
use compact_str::CompactString;
use http::StatusCode;
use serde::Deserialize;

use crate::{
    exs,
    libs::{
        auth::Session_,
        constants::{BYTES_EMPTY, BYTES_NULL},
        db::get_connection,
        privilege,
        request::JsonReqult,
        response::JkmxJsonResponse,
        serde::WithJson,
    },
    models::{
        localedict::{LocaleDict, LocaleDictEntryOwned},
        tag::{LTags, Tag},
    },
};

#[derive(Deserialize)]
struct GetAllTagsRequest {
    locale: Option<CompactString>,
}

async fn all_tags(req: JsonReqult<GetAllTagsRequest>) -> JkmxJsonResponse {
    let Json(GetAllTagsRequest { locale }) = req?;

    let mut conn = get_connection().await?;
    let tags = Tag::list(&mut conn).await?;

    let ltags = LTags { tags: tags.iter(), locale: locale.as_deref() };
    let res = format!(r#"{{"tags":{}}}"#, WithJson(ltags));
    JkmxJsonResponse::Response(StatusCode::OK, res.into())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateTagRequest {
    localized_names: Vec<LocaleDictEntryOwned>,
    color: CompactString,
}

async fn create_tag(
    Session_(session): Session_,
    req: JsonReqult<CreateTagRequest>,
) -> JkmxJsonResponse {
    let Json(CreateTagRequest { localized_names, color }) = req?;

    let mut conn = get_connection().await?;
    exs!(user, &session, &mut conn);
    if !privilege::check(&user.uid, "Lean4OJ.ManageProblem", &mut conn).await? { return JkmxJsonResponse::Response(StatusCode::FORBIDDEN, BYTES_NULL); }

    let dict = localized_names.into_iter().collect::<LocaleDict>();
    let id = Tag::create(&color, &dict, &mut conn).await?;
    let res = format!(r#"{{"id":{id}}}"#);
    JkmxJsonResponse::Response(StatusCode::OK, res.into())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateTagRequest {
    id: u32,
    localized_names: Vec<LocaleDictEntryOwned>,
    color: CompactString,
}

async fn update_tag(
    Session_(session): Session_,
    req: JsonReqult<UpdateTagRequest>,
) -> JkmxJsonResponse {
    let Json(UpdateTagRequest { id, localized_names, color }) = req?;

    let mut conn = get_connection().await?;
    exs!(user, &session, &mut conn);
    if !privilege::check(&user.uid, "Lean4OJ.ManageProblem", &mut conn).await? { return JkmxJsonResponse::Response(StatusCode::FORBIDDEN, BYTES_NULL); }

    let dict = localized_names.into_iter().collect::<LocaleDict>();
    Tag::update(id, &color, &dict, &mut conn).await?;
    JkmxJsonResponse::Response(StatusCode::OK, BYTES_EMPTY)
}

#[derive(Deserialize)]
struct DeleteTagRequest {
    id: u32,
}

async fn delete_tag(
    Session_(session): Session_,
    req: JsonReqult<DeleteTagRequest>,
) -> JkmxJsonResponse {
    let Json(DeleteTagRequest { id }) = req?;

    let mut conn = get_connection().await?;
    exs!(user, &session, &mut conn);
    if !privilege::check(&user.uid, "Lean4OJ.ManageProblem", &mut conn).await? { return JkmxJsonResponse::Response(StatusCode::FORBIDDEN, BYTES_NULL); }

    Tag::delete(id, &mut conn).await?;
    JkmxJsonResponse::Response(StatusCode::OK, BYTES_EMPTY)
}

async fn all_tags_ex() -> JkmxJsonResponse {
    let mut conn = get_connection().await?;
    let tags = Tag::list(&mut conn).await?;

    let res = format!(r#"{{"tags":{}}}"#, WithJson(tags));
    JkmxJsonResponse::Response(StatusCode::OK, res.into())
}

pub fn router() -> Router {
    Router::new()
        .route("/getAllProblemTags", post(all_tags))
        .route("/createProblemTag", post(create_tag))
        .route("/updateProblemTag", post(update_tag))
        .route("/deleteProblemTag", post(delete_tag))
        .route("/getAllProblemTagsOfAllLocales", post(all_tags_ex))
}
