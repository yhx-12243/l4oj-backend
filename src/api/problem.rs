use core::{fmt::Write, mem, str};
use std::{collections::BTreeMap, time::SystemTime};

use axum::{Extension, Json, Router, routing::post};
use bytes::Bytes;
use compact_str::CompactString;
use hashbrown::HashMap;
use http::{StatusCode, response::Parts};
use serde::Deserialize;
use serde_json::Value;
use smallvec::SmallVec;
use tokio_postgres::types::{Json as QJson, ToSql};

use crate::{
    bad, exs,
    libs::{
        auth::Session_,
        constants::{BYTES_EMPTY, BYTES_NULL},
        db::{DBError, JsonChecked, get_connection},
        lquery, privilege,
        request::JsonReqult,
        response::JkmxJsonResponse,
        serde::WithJson,
    },
    models::{
        discussion::Discussion,
        localedict::{LocaleDict, LocaleDictEntryFlatten, LocaleDictEntryOwnedFlatten},
        problem::{Problem, ProblemInner},
        tag::{LTags, Tag},
        user::User,
    },
};

mod tag;

const NO_SUCH_PROBLEM: JkmxJsonResponse = JkmxJsonResponse::Response(
    StatusCode::OK,
    Bytes::from_static(br#"{"error":"NO_SUCH_PROBLEM"}"#),
);
const NO_SUCH_USER: JkmxJsonResponse = JkmxJsonResponse::Response(
    StatusCode::OK,
    Bytes::from_static(br#"{"error":"NO_SUCH_USER"}"#),
);

mod private {
    use super::WithJson;
    use core::fmt::Write;

    pub(super) fn err() -> super::JkmxJsonResponse {
        let err = super::DBError::new(tokio_postgres::error::Kind::RowCount, Some("database problem error".into()));
        return super::JkmxJsonResponse::Error(super::StatusCode::INTERNAL_SERVER_ERROR, err.into());
    }

    #[inline]
    pub(super) fn 𝒾𝒹(kw: &str) -> Option<i32> {
        match *kw.as_bytes() {
            [b'p' | b'P', ..] => unsafe { kw.get_unchecked(1..) },
            _ => kw,
        }.parse().ok()
    }

    pub(super) fn 𝝴(res: &mut String, p: &super::Problem, locale: Option<&str>) -> std::io::Result<()> {
        if let Some((locale_key, content)) = p.content.apply_with_key(locale) {
            write!(
                res, r#"{{"meta":{},"title":{},"resultLocale":{}}},"#,
                WithJson(p),
                WithJson(&*content.title),
                WithJson(&**locale_key),
            ).map_err(std::io::Error::other)
        } else {
            Err(std::io::const_error!(std::io::ErrorKind::Other, "No content for locale"))
        }
    }

    pub(super) fn 𝝈(res: &mut String, p: &super::Problem, locale: Option<&str>, tids: &[u32], lookup: &super::HashMap<u32, Option<super::Tag>>) -> std::io::Result<()> {
        if let Some((locale_key, content)) = p.content.apply_with_key(locale) {
            let ltags = super::LTags {
                tags: tids.iter().filter_map(|tid| lookup.get(tid).and_then(Option::as_ref)),
                locale,
            };
            write!(
                res, r#"{{"meta":{},"title":{},"tags":{},"resultLocale":{}}},"#,
                WithJson(p),
                WithJson(&*content.title),
                WithJson(ltags),
                WithJson(&**locale_key),
            ).map_err(std::io::Error::other)
        } else {
            Err(std::io::const_error!(std::io::ErrorKind::Other, "No content for locale"))
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueryProblemSetRequest {
    locale: Option<CompactString>,
    keyword: Option<CompactString>,
    keyword_matches_id: Option<bool>,
    tag_ids: Option<Vec<u32>>,
    owner_id: Option<CompactString>,
    nonpublic: Option<bool>,
    title_only: Option<bool>,
    skip_count: u64,
    take_count: u64,
}

#[allow(clippy::too_many_lines)]
async fn query_problem_set(
    Session_(session): Session_,
    req: JsonReqult<QueryProblemSetRequest>
) -> JkmxJsonResponse {
    let Json(QueryProblemSetRequest {
        locale,
        keyword,
        keyword_matches_id,
        tag_ids,
        owner_id,
        nonpublic,
        title_only,
        skip_count,
        take_count,
    }) = req?;

    let keyword__inner___ = keyword.as_deref();
    let owner_id__inner___ = owner_id.as_deref();
    let buf;
    let ekw = if let Some(kw) = keyword__inner___ {
        buf = lquery::jsonb::make_jsonb_path_match_query(kw);
        Some(JsonChecked(buf.as_bytes()))
    } else { None };
    let tag_ids__inner___ = unsafe { mem::transmute::<Option<&[u32]>, Option<&'static [i32]>>(tag_ids.as_deref()) };

    let mut conn = get_connection().await?;
    let maybe_user = User::from_maybe_session(&session, &mut conn).await?;
    let uid = maybe_user.as_ref().map(|u| &*u.uid);
    let privi = if let Some(uid) = uid {
        privilege::check(uid, "Lean4OJ.ManageProblem", &mut conn).await?
    } else {
        false
    };

    let extend = |mut sql: String, mut args: SmallVec<[&'static (dyn ToSql + Sync); 8]>| -> (String, SmallVec<[&'static (dyn ToSql + Sync); 8]>) {
        if nonpublic == Some(true) {
            sql.push_str(" and is_public = false");
        }
        if let Some(ref kw) = ekw {
            let _ = write!(&mut sql, " and jsonb_path_match(pcontent, ${})", args.len() + 1);
            args.push(
                unsafe { core::mem::transmute::<&JsonChecked<'_>, &'static JsonChecked<'_>>(kw) } as _
            );
        }
        if let Some(ref owner) = owner_id__inner___ {
            let _ = write!(&mut sql, " and owner = ${}", args.len() + 1);
            args.push(
                unsafe { core::mem::transmute::<&&str, &'static &str>(owner) } as _
            );
        }
        if !privi {
            if let Some(ref uid) = uid {
                let _ = write!(&mut sql, " and (owner = ${} or is_public = true)", args.len() + 1);
                args.push(
                    unsafe { core::mem::transmute::<&&str, &'static &str>(uid) } as _
                );
            } else {
                sql.push_str(" and is_public = true");
            }
        }
        (sql, args)
    };

    let skip = skip_count.min(i64::MAX.cast_unsigned()).cast_signed();
    let take = take_count.min(100).cast_signed();

    let problems = Problem::search_aoe(skip, take, tag_ids__inner___, extend, &mut conn).await?;

    let mut res = r#"{"result":["#.to_owned();
    if title_only == Some(true) {
        if keyword_matches_id == Some(true)
        && let Some(kw) = keyword__inner___
        && let Some(pid) = private::𝒾𝒹(kw) {
            if let Some(pos) = problems.iter().position(|p| p.0.pid == pid) {
                private::𝝴(&mut res, &problems[pos].0, locale.as_deref())?;
                for (p, _) in &problems[..pos] {
                    private::𝝴(&mut res, p, locale.as_deref())?;
                }
                for (p, _) in &problems[pos + 1..] {
                    private::𝝴(&mut res, p, locale.as_deref())?;
                }
            } else {
                let problem = if privi {
                    Problem::by_pid(pid, &mut conn).await
                } else {
                    Problem::by_pid_uid(pid, uid.unwrap_or_default(), &mut conn).await
                }?;
                if let Some(ref p) = problem {
                    private::𝝴(&mut res, p, locale.as_deref())?;
                }
                for (p, _) in &problems {
                    private::𝝴(&mut res, p, locale.as_deref())?;
                }
            }
        } else {
            for (p, _) in &problems {
                private::𝝴(&mut res, p, locale.as_deref())?;
            }
        }
        if res.len() > 11 { res.pop(); }
        res.push(']');
    } else {
        let cap = problems.iter().fold(tag_ids__inner___.map_or_default(<[i32]>::len), |x, (_, y)| x + y.len());
        let mut lookup = HashMap::<u32, Option<Tag>>::with_capacity(cap);
        if let Some(ref tids) = tag_ids {
            for &tid in tids { lookup.insert(tid, None); }
        }
        for (_, tids) in &problems {
            for &tid in tids { lookup.insert(tid, None); }
        }
        Tag::get_area_of_effect(&mut lookup, &mut conn).await?;
        for (p, tids) in &problems {
            private::𝝈(&mut res, p, locale.as_deref(), tids, &lookup)?;
        }
        if res.len() > 11 { res.pop(); }
        res.push(']');

        let count = Problem::count_aoe(tag_ids__inner___, extend, &mut conn).await?;
        write!(&mut res, r#","permissions":{{"createProblem":true,"manageTags":true,"filterByOwner":true,"filterNonpublic":true}},"count":{count}"#)?;
        if let Some(ref tag_ids) = tag_ids {
            let ltags = LTags {
                tags: tag_ids.iter().filter_map(|tid| lookup.get(tid).and_then(Option::as_ref)),
                locale: locale.as_deref(),
            };
            write!(&mut res, r#","filterTags":{}"#, WithJson(ltags))?;
        }
        if let Some(owner) = owner_id__inner___ {
            res.push_str(r#","filterOwner":"#);
            let Some(user) = User::by_uid(owner, &mut conn).await? else { return NO_SUCH_USER };
            serde_json::to_writer(unsafe { res.as_mut_vec() }, &user)?;
        }
    }
    res.push('}');
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
    const SQL_PRIV: &str = "update lean4oj.problems set pcontent = $1 where pid = $2";
    const SQL: &str = "update lean4oj.problems set pcontent = $1 where pid = $2 and owner = $3";

    let Json(UpdateProblemRequest { problem_id, localized_contents, problem_tag_ids }) = req?;

    let mut conn = get_connection().await?;
    exs!(user, &session, &mut conn);

    let content = localized_contents.into_iter().collect::<LocaleDict<_>>();
    let content: &QJson<BTreeMap<CompactString, ProblemInner>> = unsafe { &*(&raw const content.0).cast() };

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
    discussion_count: Option<bool>,
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
        discussion_count,
        permission_of_current_user,
        permissions,
        last_submission_and_last_accepted_submission,
    }) = req?;
    let Some(id) = id.or(display_id) else { bad!(BYTES_NULL) };

    let mut conn = get_connection().await?;
    let maybe_user = User::from_maybe_session(&session, &mut conn).await?;
    let uid = maybe_user.as_ref().map(|u| &*u.uid);
    let privi = if let Some(uid) = uid {
        privilege::check(uid, "Lean4OJ.ManageProblem", &mut conn).await?
    } else {
        false
    };

    let Some(problem) = (
        if privi {
            Problem::by_pid(id, &mut conn).await
        } else {
            Problem::by_pid_uid(id, uid.unwrap_or_default(), &mut conn).await
        }
    )? else { return NO_SUCH_PROBLEM };

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
            let ltags = LTags { tags: tags.iter(), locale: Some(&locale) };
            write!(&mut res, r#","tagsOfLocale":{}"#, WithJson(ltags))?;
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

    if discussion_count == Some(true) {
        let n = Discussion::count_pid(id, &mut conn).await?;
        write!(&mut res, r#","discussionCount":{n}"#)?;
    }

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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateJudgeInfoRequest {
    problem_id: i32,
    judge_info: serde_json::Map<String, Value>,
    submittable: bool,
}

async fn update_judge_info(
    Session_(session): Session_,
    req: JsonReqult<UpdateJudgeInfoRequest>,
) -> JkmxJsonResponse {
    const SQL_PRIV: &str = "update lean4oj.problems set jb = $1, submittable = $2 where pid = $3";
    const SQL: &str = "update lean4oj.problems set jb = $1, submittable = $2 where pid = $3 and owner = $4";

    let Json(UpdateJudgeInfoRequest { problem_id, judge_info, submittable }) = req?;

    let mut conn = get_connection().await?;
    exs!(user, &session, &mut conn);

    let n = if privilege::check(&user.uid, "Lean4OJ.ManageProblem", &mut conn).await? {
        let stmt = conn.prepare_static(SQL_PRIV.into()).await?;
        conn.execute(&stmt, &[&QJson(judge_info), &submittable, &problem_id]).await
    } else {
        let stmt = conn.prepare_static(SQL.into()).await?;
        conn.execute(&stmt, &[&QJson(judge_info), &submittable, &problem_id, &&*user.uid]).await
    }?;
    if n != 1 { return private::err(); }

    JkmxJsonResponse::Response(StatusCode::OK, BYTES_EMPTY)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeleteProblemRequest {
    problem_id: i32,
}

async fn delete_problem(
    Session_(session): Session_,
    req: JsonReqult<DeleteProblemRequest>,
) -> JkmxJsonResponse {
    const SQL_PRIV: &str = "delete from lean4oj.problems where pid = $1";
    const SQL: &str = "delete from lean4oj.problems where pid = $1 and owner = $2";

    let Json(DeleteProblemRequest { problem_id }) = req?;

    let mut conn = get_connection().await?;
    exs!(user, &session, &mut conn);

    let n = if privilege::check(&user.uid, "Lean4OJ.ManageProblem", &mut conn).await? {
        let stmt = conn.prepare_static(SQL_PRIV.into()).await?;
        conn.execute(&stmt, &[&problem_id]).await
    } else {
        let stmt = conn.prepare_static(SQL.into()).await?;
        conn.execute(&stmt, &[&problem_id, &&*user.uid]).await
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
        .route("/updateProblemJudgeInfo", post(update_judge_info))
        .route("/deleteProblem", post(delete_problem))
        .merge(tag::router())
}
