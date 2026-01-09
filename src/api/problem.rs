use core::{fmt::Write, index::Last, str};
use std::time::SystemTime;

use axum::{Extension, Json, Router, routing::post};
use bytes::Bytes;
use compact_str::CompactString;
use hashbrown::HashMap;
use http::{StatusCode, response::Parts};
use serde::Deserialize;

use crate::{
    bad, exs,
    libs::{
        auth::Session_,
        constants::{BYTES_EMPTY, BYTES_NULL},
        db::{DBError, get_connection},
        privilege,
        request::JsonReqult,
        response::JkmxJsonResponse,
        serde::WithJson,
    },
    models::{
        localedict::{LocaleDict, LocaleDictEntryFlatten, LocaleDictEntryOwnedFlatten},
        problem::{Problem, ProblemInner},
        tag::Tag,
        user::User,
    },
};

mod tag;

const NO_SUCH_PROBLEM: JkmxJsonResponse = JkmxJsonResponse::Response(
    StatusCode::OK,
    Bytes::from_static(br#"{"error":"NO_SUCH_PROBLEM"}"#),
);

mod private {
    pub(super) fn err() -> super::JkmxJsonResponse {
        let err = super::DBError::new(tokio_postgres::error::Kind::RowCount, Some("database problem error".into()));
        return super::JkmxJsonResponse::Error(super::StatusCode::INTERNAL_SERVER_ERROR, err.into());
    }
}

async fn query_problem_set() -> JkmxJsonResponse {
    let res = r#"{"count":0,"result":[],"permissions":{"createProblem":true,"manageTags":true,"filterByOwner":true,"filterNonpublic":true}}"#;
    JkmxJsonResponse::Response(StatusCode::OK, res.into())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Inner1 {
    localized_contents: Vec<LocaleDictEntryOwnedFlatten<ProblemInner>>,
    problem_tag_ids: Vec<u32>,
}

#[derive(Deserialize)]
struct CreateProblemRequest {
    statement: Inner1,
}

async fn create_problem(
    Session_(session): Session_,
    req: JsonReqult<CreateProblemRequest>,
) -> JkmxJsonResponse {
    let Json(CreateProblemRequest { statement: Inner1 { localized_contents, problem_tag_ids } }) = req?;

    let mut conn = get_connection().await?;
    exs!(user, &session, &mut conn);

    let content = localized_contents.into_iter().collect::<LocaleDict<_>>();
    let pid = Problem::create(&user.uid, &content, &mut conn).await?;
    Problem::set_tags(pid, problem_tag_ids.iter().copied(), &mut conn).await?;
    let res = format!(r#"{{"id":{pid}}}"#);
    JkmxJsonResponse::Response(StatusCode::OK, res.into())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateProblemRequest {
    problem_id: i32,
    localized_contents: Vec<LocaleDictEntryOwnedFlatten<ProblemInner>>,
    problem_tag_ids: Vec<u32>,
}

async fn update_problem(
    Session_(session): Session_,
    req: JsonReqult<UpdateProblemRequest>,
) -> JkmxJsonResponse {
    const SQL_PRIV: &str = "update lean4oj.problems set content = $1 where pid = $2";
    const SQL: &str = "update lean4oj.problems set content = $1 where pid = $2 and owner = $3";

    let Json(UpdateProblemRequest { problem_id, localized_contents, problem_tag_ids }) = req?;

    let mut conn = get_connection().await?;
    exs!(user, &session, &mut conn);

    let content = localized_contents.into_iter().collect::<LocaleDict<_>>();
    let content: &tokio_postgres::types::Json<HashMap<CompactString, ProblemInner>> = unsafe { &*(&raw const content.0).cast() };

    let n = if privilege::check(&user.uid, "Lean4OJ.ManageProblem", &mut conn).await? {
        let stmt = conn.prepare_static(SQL_PRIV.into()).await?;
        conn.execute(&stmt, &[content, &problem_id]).await
    } else {
        let stmt = conn.prepare_static(SQL.into()).await?;
        conn.execute(&stmt, &[content, &problem_id, &&*user.uid]).await
    }?;
    if n != 1 { return private::err(); }
    Problem::set_tags(problem_id, problem_tag_ids.iter().copied(), &mut conn).await?;

    JkmxJsonResponse::Response(StatusCode::OK, BYTES_EMPTY)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetProblemRequest {
    id: Option<i32>,
    display_id: Option<i32>, // effectly always the same, compat with frontend only.
    owner: Option<bool>,
    localized_contents_of_locale: Option<CompactString>,
    localized_contents_of_all_locales: Option<bool>,
    tags_of_locale: Option<CompactString>,
    tags_of_all_locales: Option<bool>,
    permission_of_current_user: Option<bool>,
    permissions: Option<bool>,
    last_submission_and_last_accepted_submission: Option<bool>,
}

async fn get_problem(
    Session_(session): Session_,
    req: JsonReqult<GetProblemRequest>,
) -> JkmxJsonResponse {
    let Json(GetProblemRequest {
        id,
        display_id,
        owner,
        localized_contents_of_locale,
        localized_contents_of_all_locales,
        tags_of_locale,
        tags_of_all_locales,
        permission_of_current_user,
        permissions,
        last_submission_and_last_accepted_submission,
    }) = req?;
    let Some(id) = id.or(display_id) else { bad!(BYTES_NULL) };

    let mut conn = get_connection().await?;
    let Some(problem) = Problem::by_pid(id, &mut conn).await? else { return NO_SUCH_PROBLEM };
    // let maybe_user = User::from_maybe_session(&session, &mut conn).await?;

    let mut res = format!(r#"{{"meta":{}"#, WithJson(&problem));
    if owner == Some(true) {
        let owner_entity = User::by_uid(&problem.owner, &mut conn).await?;
        write!(&mut res, r#","owner":{}"#, WithJson(owner_entity))?;
    }

    if let Some(locale) = localized_contents_of_locale {
        let Some((locale_key, content)) = problem.content.apply_with_key(Some(&locale)) else { bad!(BYTES_NULL) };
        write!(&mut res, r#","localizedContentsOfLocale":{}"#, WithJson(LocaleDictEntryFlatten { locale: locale_key, field: content }))?;
    }

    if localized_contents_of_all_locales == Some(true) {
        write!(&mut res, r#","localizedContentsOfAllLocales":{}"#, WithJson(problem.content))?;
    }

    if tags_of_locale.is_some() || tags_of_all_locales == Some(true) {
        let tags = Tag::of_assoc_pid(id, &mut conn).await?;
        if let Some(locale) = tags_of_locale {
            res.push_str(r#","tagsOfLocale":["#);
            if tags.is_empty() {
                res.push(']');
            } else {
                for tag in &tags {
                    write!(
                        &mut res,
                        r#"{{"id":{},"color":{},"name":{}}},"#,
                        tag.id,
                        WithJson(&*tag.color),
                        WithJson(tag.name.apply(Some(&locale)).map_or_default(|x| &**x)),
                    )?;
                }
                unsafe { *res.as_mut_vec().get_unchecked_mut(Last) = b']'; }
            }
        }
        if tags_of_all_locales == Some(true) {
            write!(&mut res, r#","tagsOfAllLocales":{}"#, WithJson(tags))?;
        }
    }

    write!(
        &mut res, r#","samples":[],"judgeInfo":{},"submittable":{},"testData":[],"additionalFiles":[]"#,
        unsafe { str::from_utf8_unchecked(&problem.jb) },
        problem.submittable,
    )?;

    if permission_of_current_user == Some(true) {
        res.push_str(r#","permissionOfCurrentUser":["View","Modify","ManagePermission","ManagePublicness","Delete"]"#);
    }

    if permissions == Some(true) {
        res.push_str(r#","permissions":{"userPermissions":[],"groupPermissions":[]}"#);
    }

    if last_submission_and_last_accepted_submission == Some(true) {
        res.push_str(r#","lastSubmission":{}"#);
    }

    res.push('}');
    JkmxJsonResponse::Response(StatusCode::OK, res.into())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetProblemIdRequest {
    problem_id: i32, // old ID
    display_id: i32, // new ID
}

async fn set_problem_id(
    Session_(session): Session_,
    req: JsonReqult<SetProblemIdRequest>,
) -> JkmxJsonResponse {
    const SQL: &str = "update lean4oj.problems set pid = $1 where pid = $2";

    let Json(SetProblemIdRequest { problem_id, display_id }) = req?;

    if display_id == 0 || display_id == i32::MIN || problem_id == 0 || problem_id == i32::MIN { bad!(BYTES_NULL); }
    if display_id == problem_id {
        return JkmxJsonResponse::Response(StatusCode::OK, BYTES_EMPTY);
    }

    let mut conn = get_connection().await?;
    exs!(user, &session, &mut conn);
    if !privilege::check(&user.uid, "Lean4OJ.ManageProblem", &mut conn).await? {
        return JkmxJsonResponse::Response(StatusCode::FORBIDDEN, BYTES_EMPTY);
    }

    let stmt = conn.prepare_static(SQL.into()).await?;
    let n = conn.execute(&stmt, &[&display_id, &problem_id]).await?;
    if n != 1 { return private::err(); }

    JkmxJsonResponse::Response(StatusCode::OK, BYTES_EMPTY)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetProblemPublicnessRequest {
    problem_id: i32,
    is_public: bool,
}

async fn set_problem_publicness(
    Extension(now): Extension<SystemTime>,
    Session_(session): Session_,
    req: JsonReqult<SetProblemPublicnessRequest>,
) -> JkmxJsonResponse {
    const SQL_PUBLIC: &str = "update lean4oj.problems set is_public = true, public_at = $1 where pid = $2 and is_public = false";
    const SQL_PRIVATE: &str = "update lean4oj.problems set is_public = false where pid = $1 and is_public = true";

    let Json(SetProblemPublicnessRequest { problem_id, is_public }) = req?;

    let mut conn = get_connection().await?;
    exs!(user, &session, &mut conn);
    if !privilege::check(&user.uid, "Lean4OJ.ManageProblem", &mut conn).await? {
        return JkmxJsonResponse::Response(StatusCode::FORBIDDEN, BYTES_EMPTY);
    }

    let n = if is_public {
        let stmt = conn.prepare_static(SQL_PUBLIC.into()).await?;
        conn.execute(&stmt, &[&now, &problem_id]).await
    } else {
        let stmt = conn.prepare_static(SQL_PRIVATE.into()).await?;
        conn.execute(&stmt, &[&problem_id]).await
    }?;
    if n != 1 { return private::err(); }

    JkmxJsonResponse::Response(StatusCode::OK, BYTES_EMPTY)
}

pub fn router(_header: &'static Parts) -> Router {
    Router::new()
        .route("/queryProblemSet", post(query_problem_set))
        .route("/createProblem", post(create_problem))
        .route("/updateStatement", post(update_problem))
        .route("/getProblem", post(get_problem))
        .route("/setProblemDisplayId", post(set_problem_id))
        .route("/setProblemPublic", post(set_problem_publicness))
        .merge(tag::router())
}
