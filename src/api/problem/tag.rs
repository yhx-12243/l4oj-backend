use core::{fmt::Write, index::Last};

use axum::{Json, Router, routing::post};
use compact_str::CompactString;
use http::StatusCode;
use serde::Deserialize;

use crate::{
    libs::{auth::Session_, constants::BYTES_NULL, db::get_connection, request::JsonReqult, response::JkmxJsonResponse, serde::WithJson},
    models::{
        localedict::{LocaleDict, LocaleDictEntryOwned},
        tag::Tag, user::User,
    },
};

#[derive(Deserialize)]
pub struct GetAllTagsRequest {
    locale: Option<CompactString>,
}

async fn all_tags(req: JsonReqult<GetAllTagsRequest>) -> JkmxJsonResponse {
    let Json(GetAllTagsRequest { locale }) = req?;
    let locale = locale.as_deref();

    let mut conn = get_connection().await?;
    let tags = Tag::list(&mut conn).await?;
    let mut res = r#"{"tags":["#.to_owned();
    for tag in tags {
        write!(
            &mut res,
            r#"{{"id":{},"color":{},"name":{}}},"#,
            tag.id,
            WithJson(&*tag.color),
            WithJson(tag.name.apply(locale)),
        )?;
    }
    let mut res = res.into_bytes();
    unsafe { *res.get_unchecked_mut(Last) = b']'; }
    res.push(b'}');
    JkmxJsonResponse::Response(StatusCode::OK, res.into())
}

async fn all_tags_ex() -> JkmxJsonResponse {
    let mut conn = get_connection().await?;
    let tags = Tag::list(&mut conn).await?;

    let res = format!(r#"{{"tags":{}}}"#, WithJson(tags));
    JkmxJsonResponse::Response(StatusCode::OK, res.into())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateTagRequest {
    color: CompactString,
    localized_names: Vec<LocaleDictEntryOwned>,
}

async fn create_tag(
    Session_(session): Session_,
    req: JsonReqult<CreateTagRequest>,
) -> JkmxJsonResponse {
    let Json(CreateTagRequest { color, localized_names }) = req?;
    let Some(session) = session else { return JkmxJsonResponse::Response(StatusCode::FORBIDDEN, BYTES_NULL); };

    let mut conn = get_connection().await?;
    let Some(user) = User::from_session(&session, &mut conn).await? else { return JkmxJsonResponse::Response(StatusCode::FORBIDDEN, BYTES_NULL) };

    let dict = localized_names.into_iter().collect::<LocaleDict>();
    let id = Tag::create(&color, &dict, &mut conn).await?;
    let res = format!(r#"{{"id":{id}}}"#);
    JkmxJsonResponse::Response(StatusCode::OK, res.into())
}

pub fn router() -> Router {
    Router::new()
        .route("/getAllProblemTags", post(all_tags))
        .route("/getAllProblemTagsOfAllLocales", post(all_tags_ex))
        .route("/createProblemTag", post(create_tag))
}
