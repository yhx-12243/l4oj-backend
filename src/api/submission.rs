use core::{fmt::Write, mem::MaybeUninit, str};
use std::time::SystemTime;

use axum::{Extension, Json, Router, routing::post};
use bytes::Bytes;
use compact_str::CompactString;
use http::{StatusCode, response::Parts};
use openssl::sha::Sha256;
use serde::Deserialize;
use smallvec::SmallVec;
use tokio_postgres::types::ToSql;

use crate::{
    bad, exs,
    libs::{
        auth::Session_,
        constants::{BYTES_EMPTY, BYTES_NULL},
        db::{DBError, get_connection},
        olean, privilege,
        request::JsonReqult,
        response::JkmxJsonResponse,
        serde::WithJson,
        util::hex_digit,
        validate::is_lean_id,
    },
    models::{
        problem::Problem,
        submission::{Submission, SubmissionAoe, SubmissionMeta, SubmissionStatus},
        user::User,
    },
    service::submission_deposit,
};

const NO_SUCH_SUBMISSION: JkmxJsonResponse = JkmxJsonResponse::Response(
    StatusCode::OK,
    Bytes::from_static(br#"{"error":"NO_SUCH_SUBMISSION"}"#),
);

mod private {
    pub(super) fn err() -> super::JkmxJsonResponse {
        let err = super::DBError::new(tokio_postgres::error::Kind::RowCount, Some("database submission error".into()));
        return super::JkmxJsonResponse::Error(super::StatusCode::INTERNAL_SERVER_ERROR, err.into());
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetOleanMetaRequest {
    module_name: CompactString,
}

async fn get_olean_meta(
    Session_(session): Session_,
    req: JsonReqult<GetOleanMetaRequest>,
) -> JkmxJsonResponse {
    const EMPTY: JkmxJsonResponse = JkmxJsonResponse::Response(StatusCode::OK, Bytes::from_static(br#"{"consts":[],"dependencies":[]}"#));

    let Json(GetOleanMetaRequest { module_name }) = req?;

    if !module_name.split('.').all(is_lean_id) { bad!(BYTES_NULL); }

    let mut conn = get_connection().await?;
    exs!(user, &session, &mut conn);

    let olean_path = olean::𝑔𝑒𝑡_𝑜𝑙𝑒𝑎𝑛_𝑝𝑎𝑡ℎ(&user.uid, &module_name);

    let Ok(olean) = tokio::fs::read(&*olean_path).await else { return EMPTY };
    let Some(meta) = olean::parse_meta(&olean) else { return EMPTY };
    let Some(consts) = olean::parse_consts(meta) else { return EMPTY };
    let Some(dependencies) = olean::parse_imports(meta) else { return EMPTY };

    let res = format!(r#"{{"consts":{},"dependencies":{}}}"#, WithJson(&*consts), WithJson(&*dependencies));
    JkmxJsonResponse::Response(StatusCode::OK, res.into())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Inner1 {
    module_name: CompactString,
    const_name: CompactString,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubmitRequest {
    problem_id: i32,
    content: Inner1,
}

async fn submit(
    Extension(now): Extension<SystemTime>,
    Session_(session): Session_,
    req: JsonReqult<SubmitRequest>,
) -> JkmxJsonResponse {
    const SQL_SEL_PRIV: &str = "select * from lean4oj.problems where pid = $1 and submittable";
    const SQL_SEL: &str = "select * from lean4oj.problems where pid = $1 and (owner = $2 or is_public) and submittable";
    const SQL_ADD_SUB: &str = "update lean4oj.problems set sub = sub + 1 where pid = $1";

    let Json(SubmitRequest { problem_id, content: Inner1 { module_name, const_name } }) = req?;

    if !module_name.split('.').all(is_lean_id) || !const_name.split('.').all(is_lean_id) { bad!(BYTES_NULL); }

    let mut conn = get_connection().await?;
    exs!(user, &session, &mut conn);

    let problem: Problem = if privilege::check(&user.uid, "Lean4OJ.ManageProblem", &mut conn).await? {
        let stmt = conn.prepare_static(SQL_SEL_PRIV.into()).await?;
        conn.query_one(&stmt, &[&problem_id]).await
    } else {
        let stmt = conn.prepare_static(SQL_SEL.into()).await?;
        conn.query_one(&stmt, &[&problem_id, &&*user.uid]).await
    }?.try_into()?;

    let olean_path = olean::𝑔𝑒𝑡_𝑜𝑙𝑒𝑎𝑛_𝑝𝑎𝑡ℎ(&user.uid, &module_name);

    let olean = tokio::fs::read(&*olean_path).await?;
    let Some(meta) = olean::parse_meta(&olean) else { bad!(BYTES_NULL) };
    let Some(consts) = olean::parse_consts(meta) else { bad!(BYTES_NULL) };
    let Some(imports) = olean::parse_imports(meta) else { bad!(BYTES_NULL) };
    if !consts.contains(&const_name) { bad!(BYTES_NULL); }

    let mut sha256 = Sha256::new();
    sha256.update(&olean);
    let answer_hash = sha256.finish();

    let sid = Submission::create(problem_id, &user.uid, now,
        &module_name, &const_name, meta.version,
        olean.len() as u64, answer_hash,
        &mut conn,
    ).await?;

    let stmt = conn.prepare_static(SQL_ADD_SUB.into()).await?;
    let n = conn.execute(&stmt, &[&problem_id]).await?;
    if n != 1 { return private::err(); }

    let task = submission_deposit::Task {
        sid,
        uid: user.uid,
        module_name,
        const_name,
        imports,
        hash: answer_hash,
        checker: problem.jb,
    };
    submission_deposit::transmit(task)?;

    let res = format!(r#"{{"submissionId":{sid}}}"#);
    JkmxJsonResponse::Response(StatusCode::OK, res.into())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuerySubmissionRequest {
    locale: Option<CompactString>,
    problem_id: Option<i32>,
    problem_display_id: Option<i32>, // effectly always the same, compat with frontend only.
    submitter: Option<CompactString>,
    lean_version: Option<CompactString>,
    status: Option<SubmissionStatus>,
    min_id: Option<u32>,
    max_id: Option<u32>,
    take_count: u64,
}

async fn query_submission(
    Session_(session): Session_,
    req: JsonReqult<QuerySubmissionRequest>,
) -> JkmxJsonResponse {
    let Json(QuerySubmissionRequest {
        locale,
        problem_id,
        problem_display_id,
        submitter,
        lean_version,
        status,
        min_id,
        max_id,
        take_count,
    }) = req?;

    let pid = problem_id.or(problem_display_id);
    let aoe_style = match (min_id, max_id) {
        (Some(_), Some(_)) => bad!(BYTES_NULL),
        (Some(min_id), None) => SubmissionAoe::After(min_id),
        (None, Some(max_id)) => SubmissionAoe::Before(max_id),
        (None, None) => SubmissionAoe::Global,
    };

    let submitter__inner___ = submitter.as_deref();
    let lean_version__inner___ = if let Some(ref v) = lean_version {
        if let Some(stripped) = v.strip_prefix('4') { Some(stripped) } else { bad!(BYTES_NULL) }
    } else {
        None
    };

    let mut conn = get_connection().await?;
    let maybe_user = User::from_maybe_session(&session, &mut conn).await?;
    let uid = maybe_user.as_ref().map(|u| &*u.uid);
    let privi = if let Some(uid) = uid {
        privilege::check(uid, "Lean4OJ.ManageProblem", &mut conn).await?
    } else {
        false
    };

    let extend = |mut sql: String, mut args: SmallVec<[&'static (dyn ToSql + Sync); 8]>| -> (String, SmallVec<[&'static (dyn ToSql + Sync); 8]>) {
        if let Some(ref pid) = pid {
            let _ = write!(&mut sql, " and pid = ${}", args.len() + 1);
            args.push(
                unsafe { core::mem::transmute::<&i32, &'static i32>(pid) } as _
            );
        }
        if let Some(ref submitter) = submitter__inner___ {
            let _ = write!(&mut sql, " and submitter = ${}", args.len() + 1);
            args.push(
                unsafe { core::mem::transmute::<&&str, &'static &str>(submitter) } as _
            );
        }
        if let Some(ref lean_version) = lean_version__inner___ {
            let _ = write!(&mut sql, " and lean_toolchain = ${}", args.len() + 1);
            args.push(
                unsafe { core::mem::transmute::<&&str, &'static &str>(lean_version) } as _
            );
        }
        if let Some(ref status) = status {
            let _ = write!(&mut sql, " and status = ${}", args.len() + 1);
            args.push(
                unsafe { core::mem::transmute::<&SubmissionStatus, &'static i8>(status) } as _
            );
        }
        if !privi {
            if let Some(ref uid) = uid {
                let _ = write!(&mut sql, " and (owner = ${} or is_public)", args.len() + 1);
                args.push(
                    unsafe { core::mem::transmute::<&&str, &'static &str>(uid) } as _
                );
            } else {
                sql.push_str(" and is_public");
            }
        }
        (sql, args)
    };

    let take = take_count.min(100).cast_signed();

    let submissions = Submission::search_aoe(take + 1, aoe_style, extend, &mut conn).await?;
    let is_excess = submissions.len() as i64 == take + 1;

    let (has_left, has_right) = match aoe_style {
        SubmissionAoe::Global => (is_excess, false),
        SubmissionAoe::After(_) => (Submission::ping_one(aoe_style, extend, &mut conn).await?, is_excess),
        SubmissionAoe::Before(_) => (is_excess, Submission::ping_one(aoe_style, extend, &mut conn).await?),
    };

    let mut res = r#"{"submissions":["#.to_owned();
    let iter: &mut dyn Iterator<Item = (Submission, Problem, User)> = match aoe_style {
        SubmissionAoe::After(_) => &mut submissions.into_iter().take(take as usize).rev(),
        _ => &mut submissions.into_iter().take(take as usize),
    };
    for (submission, problem, submitter) in iter {
        let meta = SubmissionMeta {
            submission,
            problem,
            submitter,
            locale: locale.as_deref(),
        };
        serde_json::to_writer(unsafe { res.as_mut_vec() }, &meta).unwrap();
        res.push(',');
    }
    if res.len() > 16 { res.pop(); }
    write!(&mut res, r#"],"hasSmallerId":{has_left},"hasLargerId":{has_right}}}"#).unwrap();
    JkmxJsonResponse::Response(StatusCode::OK, res.into())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetSubmissionRequest {
    locale: Option<CompactString>,
    submission_id: u32,
}

async fn get_submission(
    Session_(session): Session_,
    req: JsonReqult<GetSubmissionRequest>,
) -> JkmxJsonResponse {
    let Json(GetSubmissionRequest { locale, submission_id }) = req?;

    let mut conn = get_connection().await?;
    let maybe_user = User::from_maybe_session(&session, &mut conn).await?;
    let uid = maybe_user.as_ref().map(|u| &*u.uid);
    let privi = if let Some(uid) = uid {
        privilege::check(uid, "Lean4OJ.ManageProblem", &mut conn).await?
    } else {
        false
    };

    let Some((submission, problem, submitter)) = if privi {
        Submission::by_sid_with_problem(submission_id, &mut conn).await
    } else {
        Submission::by_sid_uid_with_problem(submission_id, uid.unwrap_or_default(), &mut conn).await
    }? else { return NO_SUCH_SUBMISSION };

    let mut hash = [MaybeUninit::<u8>::uninit(); 64];
    for (i, &x) in submission.answer_hash.iter().enumerate() {
        hash[2 * i].write(hex_digit(x >> 4).into());
        hash[2 * i + 1].write(hex_digit(x & 15).into());
    }

    let lean_version = submission.lean_toolchain.clone();

    let meta = SubmissionMeta {
        submission,
        problem,
        submitter,
        locale: locale.as_deref(),
    };

    let res = format!(
        r#"{{"meta":{},"content":{{"hash":"{}","leanVersion":"4{lean_version}"}},"permissionRejudge":true,"permissionCancel":true,"permissionSetPublic":true,"permissionDelete":true}}"#,
        WithJson(meta),
        unsafe { str::from_utf8_unchecked(hash.assume_init_ref()) },
    );
    JkmxJsonResponse::Response(StatusCode::OK, res.into())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SingleSubmissionRequest {
    submission_id: u32,
}

async fn rejudge_submission(
    Session_(session): Session_,
    req: JsonReqult<SingleSubmissionRequest>,
) -> JkmxJsonResponse {
    const SQL_PRIV: &str = "select sid, pid, submitter, submit_time, module_name, const_name, lean_toolchain, status, message, answer_size, answer_hash, answer_obj, is_public, public_at, owner, pcontent, sub, pac, submittable, jb from lean4oj.submissions natural join lean4oj.problems where sid = $1 and status::integer >= 7";
    const SQL: &str = "select sid, pid, submitter, submit_time, module_name, const_name, lean_toolchain, status, message, answer_size, answer_hash, answer_obj, is_public, public_at, owner, pcontent, sub, pac, submittable, jb from lean4oj.submissions natural join lean4oj.problems where sid = $1 and status::integer >= 7 and owner = $2";
    const SQL_REJUDGE: &str = "update lean4oj.submissions set lean_toolchain = $1, status = 0::\"char\", answer_size = $2, answer_hash = $3, answer_obj = '' where sid = $4";
    const SQL_REJUDGE_FAIL: &str = "update lean4oj.submissions set status = '\x07', message = $1 where sid = $2";

    let Json(SingleSubmissionRequest { submission_id }) = req?;

    let mut conn = get_connection().await?;
    exs!(user, &session, &mut conn);

    let row = if privilege::check(&user.uid, "Lean4OJ.ManageProblem", &mut conn).await? {
        let stmt = conn.prepare_static(SQL_PRIV.into()).await?;
        conn.query_opt(&stmt, &[&submission_id.cast_signed()]).await
    } else {
        let stmt = conn.prepare_static(SQL.into()).await?;
        conn.query_opt(&stmt, &[&submission_id.cast_signed(), &&*user.uid]).await
    }?;
    let (submission, problem) = match row {
        Some(row) => (Submission::try_from(row.clone())?, Problem::try_from(row)?),
        None => return NO_SUCH_SUBMISSION,
    };

    /******** re-fetch files ********/
    let olean_path = olean::𝑔𝑒𝑡_𝑜𝑙𝑒𝑎𝑛_𝑝𝑎𝑡ℎ(&submission.submitter, &submission.module_name);

    let w: Option<_> = try {
        let olean = tokio::fs::read(&*olean_path).await.ok()?;
        let meta = olean::parse_meta(&olean)?;
        let consts = olean::parse_consts(meta)?;
        let imports = olean::parse_imports(meta)?;
        if !consts.contains(&submission.const_name) { do yeet; }
        let version = meta.version;
        (olean, version, imports)
    };
    let (olean, version, imports) = match w {
        Some(w) => w,
        None => {
            let stmt = conn.prepare_static(SQL_REJUDGE_FAIL.into()).await?;
            let n = conn.execute(&stmt, &[&"Rejudge fail.", &submission_id.cast_signed()]).await?;
            return if n == 1 { JkmxJsonResponse::Response(StatusCode::OK, BYTES_EMPTY) } else { private::err() };
        }
    };

    let mut sha256 = Sha256::new();
    sha256.update(&olean);
    let answer_hash = sha256.finish();

    let task = submission_deposit::Task {
        sid: submission_id,
        uid: submission.submitter,
        module_name: submission.module_name,
        const_name: submission.const_name,
        imports,
        hash: answer_hash,
        checker: problem.jb,
    };

    let mut path0 = String::with_capacity(env!("OLEAN_ROOT").len() + 24);
    path0.push_str(env!("OLEAN_ROOT"));
    path0.push_str("/submissions/");
    let bytes = submission_id.to_le_bytes();
    let _ = write!(&mut path0, "{:02x}/{:02x}/{:02x}/{:02x}", bytes[3], bytes[2], bytes[1], bytes[0]);
    tokio::fs::remove_dir_all(&*path0).await?;

    let stmt = conn.prepare_static(SQL_REJUDGE.into()).await?;
    let n = conn.execute(&stmt, &[
        &version, &(olean.len() as i64), &answer_hash.as_slice(), &submission_id.cast_signed(),
    ]).await?;
    if n != 1 { return private::err(); }

    submission_deposit::transmit(task)?;

    JkmxJsonResponse::Response(StatusCode::OK, BYTES_EMPTY)
}

async fn cancel_submission(
    Session_(session): Session_,
    req: JsonReqult<SingleSubmissionRequest>,
) -> JkmxJsonResponse {
    const SQL_CANCEL: &str = "update lean4oj.submissions set status = '\x0b' where sid = $1 and status::integer >= 7";

    let Json(SingleSubmissionRequest { submission_id }) = req?;

    let mut conn = get_connection().await?;
    exs!(user, &session, &mut conn);

    if !privilege::check(&user.uid, "Lean4OJ.ManageProblem", &mut conn).await? {
        return JkmxJsonResponse::Response(StatusCode::FORBIDDEN, BYTES_EMPTY);
    }

    let n = conn.execute(SQL_CANCEL, &[&submission_id.cast_signed()]).await?;
    if n != 1 { return NO_SUCH_SUBMISSION; }

    JkmxJsonResponse::Response(StatusCode::OK, BYTES_EMPTY)
}

async fn delete_submission(
    Session_(session): Session_,
    req: JsonReqult<SingleSubmissionRequest>,
) -> JkmxJsonResponse {
    const SQL_DELETE: &str = "delete from lean4oj.submissions where sid = $1 returning pid";
    const SQL_REDUCE: &str = "update lean4oj.problems set sub = sub - 1 where pid = $1";

    let Json(SingleSubmissionRequest { submission_id }) = req?;

    let mut conn = get_connection().await?;
    exs!(user, &session, &mut conn);

    if !privilege::check(&user.uid, "Lean4OJ.ManageProblem", &mut conn).await? {
        return JkmxJsonResponse::Response(StatusCode::FORBIDDEN, BYTES_EMPTY);
    }

    let stmt_delete = conn.prepare_static(SQL_DELETE.into()).await?;
    let stmt_reduce = conn.prepare_static(SQL_REDUCE.into()).await?;
    let txn = conn.transaction().await?;
    let row = txn.query_one(&stmt_delete, &[&submission_id.cast_signed()]).await?;
    let pid = row.try_get::<_, i32>(0)?;
    let n = txn.execute(&stmt_reduce, &[&pid]).await?;
    if n != 1 { return private::err(); }
    txn.commit().await?;

    JkmxJsonResponse::Response(StatusCode::OK, BYTES_EMPTY)
}

pub fn router(_header: &'static Parts) -> Router {
    Router::new()
        .route("/getOleanMeta", post(get_olean_meta))
        .route("/submit", post(submit))
        .route("/querySubmission", post(query_submission))
        .route("/getSubmissionDetail", post(get_submission))
        .route("/rejudgeSubmission", post(rejudge_submission))
        .route("/cancelSubmission", post(cancel_submission))
        .route("/deleteSubmission", post(delete_submission))
}
