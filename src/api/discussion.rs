use core::{fmt::Write, mem};
use std::time::SystemTime;

use axum::{
    Extension, Json, Router,
    routing::{post, post_service},
};
use bytes::Bytes;
use compact_str::CompactString;
use hashbrown::HashMap;
use http::{StatusCode, response::Parts};
use serde::{
    Deserialize, Serialize, Serializer,
    ser::{SerializeMap, SerializeSeq},
};
use smallvec::SmallVec;
use tokio_postgres::types::ToSql;

use crate::{
    exs,
    libs::{
        auth::Session_,
        constants::BYTES_EMPTY,
        db::{DBError, get_connection},
        emoji,
        lquery::𝑒𝑠𝑐𝑎𝑝𝑒,
        privilege,
        request::{JsonReqult, RawPayload},
        response::JkmxJsonResponse,
        serde::WithJson,
        util::get_millis,
        validate::is_lean_id,
    },
    models::{
        discussion::{
            Discussion, DiscussionReactionAOE, DiscussionReactionType, DiscussionReply,
            DiscussionReplyAOE, QueryRepliesType, REPLY_PERMISSION_DEFAULT, reaction_aoe,
        },
        problem::Problem,
        user::{User, UserAOE},
    },
};

const INVALID_EMOJI: JkmxJsonResponse = JkmxJsonResponse::Response(
    StatusCode::OK,
    Bytes::from_static(br#"{"error":"INVALID_EMOJI"}"#),
);
const NO_FLAGS: JkmxJsonResponse = JkmxJsonResponse::Response(
    StatusCode::OK,
    Bytes::from_static(br#"{"error":"NO_FLAGS"}"#),
);
const NO_SUCH_DISCUSSION: JkmxJsonResponse = JkmxJsonResponse::Response(
    StatusCode::OK,
    Bytes::from_static(br#"{"error":"NO_SUCH_DISCUSSION"}"#),
);
const NO_SUCH_DISCUSSION_REPLY: JkmxJsonResponse = JkmxJsonResponse::Response(
    StatusCode::OK,
    Bytes::from_static(br#"{"error":"NO_SUCH_DISCUSSION_REPLY"}"#),
);
const NO_SUCH_USER: JkmxJsonResponse = JkmxJsonResponse::Response(
    StatusCode::OK,
    Bytes::from_static(br#"{"error":"NO_SUCH_USER"}"#),
);

mod private {
    pub(super) fn err() -> super::JkmxJsonResponse {
        let err = super::DBError::new(tokio_postgres::error::Kind::RowCount, Some("database discussion error".into()));
        return super::JkmxJsonResponse::Error(super::StatusCode::INTERNAL_SERVER_ERROR, err.into());
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateDiscussionRequest {
    problem_id: Option<i32>,
    title: CompactString,
    content: CompactString,
}

async fn create_discussion(
    Extension(now): Extension<SystemTime>,
    Session_(session): Session_,
    req: JsonReqult<CreateDiscussionRequest>,
) -> JkmxJsonResponse {
    let Json(CreateDiscussionRequest { problem_id, title, content }) = req?;

    let mut conn = get_connection().await?;
    exs!(user, &session, &mut conn);

    let id = Discussion::create(problem_id, &title, &content, now, &user.uid, &mut conn).await?;
    let res = format!(r#"{{"discussionId":{id}}}"#);
    JkmxJsonResponse::Response(StatusCode::OK, res.into())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateReplyRequest {
    discussion_id: u32,
    content: CompactString,
}

async fn create_reply(
    Extension(now): Extension<SystemTime>,
    Session_(session): Session_,
    req: JsonReqult<CreateReplyRequest>,
) -> JkmxJsonResponse {
    const SQL_CREATE_REPLY: &str = "insert into lean4oj.discussion_replies (content, publish, edit, did, publisher) values ($1, $2, $2, $3, $4) returning id";
    const SQL_UPDATE_PARENT: &str = "update lean4oj.discussions set update = $1, reply_count = reply_count + 1 where id = $2";

    let Json(CreateReplyRequest { discussion_id, content }) = req?;

    let mut conn = get_connection().await?;
    exs!(user, &session, &mut conn);

    let stmt_create_reply = conn.prepare_static(SQL_CREATE_REPLY.into()).await?;
    let stmt_update_parent = conn.prepare_static(SQL_UPDATE_PARENT.into()).await?;
    let txn = conn.transaction().await?;
    let row = txn.query_one(&stmt_create_reply, &[&&*content, &now, &discussion_id.cast_signed(), &&*user.uid]).await?;
    let id = row.try_get::<_, i32>(0)?.cast_unsigned();
    let n = txn.execute(&stmt_update_parent, &[&now, &discussion_id.cast_signed()]).await?;
    if n != 1 { return private::err(); }
    txn.commit().await?;

    let privi = privilege::all(&user.uid, &mut conn).await?;
    let reply = DiscussionReply {
        id,
        content,
        publish: now,
        edit: now,
        did: discussion_id,
        publisher: user.uid.clone(),
    };
    let user_aoe = UserAOE {
        user,
        is_admin: privilege::is_admin(&privi),
        is_problem_admin: privi.iter().any(|p| p == "ManageProblem"),
        is_contest_admin: privi.iter().any(|p| p == "ManageContest"),
        is_discussion_admin: privi.iter().any(|p| p == "ManageDiscussion"),
    };
    let aoe = DiscussionReplyAOE {
        reply: &reply,
        publisher: Some(&user_aoe),
        reactions: Some(&DiscussionReactionAOE::default()),
        permissions: REPLY_PERMISSION_DEFAULT,
    };
    let res = format!(r#"{{"reply":{}}}"#, WithJson(aoe));
    JkmxJsonResponse::Response(StatusCode::OK, res.into())
}

#[derive(Deserialize)]
struct ReactionRequest {
    r#type: DiscussionReactionType,
    id: u32,
    emoji: CompactString,
    reaction: bool,
}

async fn reaction(
    Session_(session): Session_,
    req: JsonReqult<ReactionRequest>,
) -> JkmxJsonResponse {
    const SQL_EXIST_D: &str = "select 1 from lean4oj.discussions where id = $1";
    const SQL_EXIST_R: &str = "select 1 from lean4oj.discussion_replies where id = $1";
    const SQL_REACT_ADD: &str = "insert into lean4oj.discussion_reactions (eid, uid, emoji) values ($1, $2, $3)";
    const SQL_REACT_DEL: &str = "delete from lean4oj.discussion_reactions where eid = $1 and uid = $2 and emoji = $3";

    let Json(ReactionRequest { r#type: ty, id, emoji, reaction }) = req?;

    if emoji.chars().any(|ch| matches!(ch, '🇦'..='🇿')) { return NO_FLAGS; }
    let Some(emoji) = emoji::normalize(&emoji) else { return INVALID_EMOJI };

    let mut conn = get_connection().await?;
    exs!(user, &session, &mut conn);

    let eid = match ty {
        DiscussionReactionType::Discussion => {
            let stmt = conn.prepare_static(SQL_EXIST_D.into()).await?;
            if conn.query_opt(&stmt, &[&id.cast_signed()]).await?.is_none() { return NO_SUCH_DISCUSSION; }
            id
        }
        DiscussionReactionType::DiscussionReply => {
            let stmt = conn.prepare_static(SQL_EXIST_R.into()).await?;
            if conn.query_opt(&stmt, &[&id.cast_signed()]).await?.is_none() { return NO_SUCH_DISCUSSION_REPLY; }
            !id
        }
    }.cast_signed();

    let stmt = conn.prepare_static(if reaction { SQL_REACT_ADD } else { SQL_REACT_DEL }.into()).await?;
    let n = conn.execute(&stmt, &[&eid, &&*user.uid, &&*emoji]).await?;
    if n != 1 { return private::err(); }

    let res = format!(r#"{{"normalized":"{emoji}"}}"#); // emoji never needs escape.
    JkmxJsonResponse::Response(StatusCode::OK, res.into())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueryDiscussionRequest {
    locale: Option<CompactString>,
    keyword: Option<CompactString>,
    problem_id: Option<i32>, // `null` for global. `0` for ALL problems.
    publisher_id: Option<CompactString>,
    title_only: Option<bool>,
    skip_count: u64,
    take_count: u64,
}

#[derive(Serialize)]
#[repr(transparent)]
struct Inner1 {
    meta: Discussion,
}

struct Inner2<'a> {
    problem: Problem,
    locale: Option<&'a str>,
}

impl Serialize for Inner2<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer
    {
        let mut map = serializer.serialize_map(Some(3))?;
        map.serialize_entry("meta", &self.problem)?;
        if let Some((local_key, content)) = self.problem.content.apply_with_key(self.locale) {
            map.serialize_entry("title", &*content.title)?;
            map.serialize_entry("titleLocale", &**local_key)?;
        } else {
            map.serialize_entry("title", &())?;
            map.serialize_entry("titleLocale", &())?;
        }
        map.end()
    }
}

#[derive(Serialize)]
struct Inner3<'a> {
    meta: Discussion,
    problem: Option<Inner2<'a>>,
    publisher: User,
}

#[allow(clippy::too_many_lines)]
async fn query_discussions(
    Session_(session): Session_,
    req: JsonReqult<QueryDiscussionRequest>,
) -> JkmxJsonResponse {
    let Json(QueryDiscussionRequest {
        locale,
        keyword,
        problem_id,
        publisher_id,
        title_only,
        skip_count,
        take_count,
    }) = req?;

    let kw = keyword.as_deref().unwrap_or_default();
    let uid = publisher_id.as_deref().unwrap_or_default();
    let has_kw = !kw.is_empty();
    let has_uid = is_lean_id(uid);
    let buf;
    let ekw = if has_kw { buf = 𝑒𝑠𝑐𝑎𝑝𝑒(kw); &*buf } else { kw };

    let mut res = r#"{"discussions":"#.to_owned();
    let mut conn = get_connection().await?;
    let maybe_user = User::from_maybe_session(&session, &mut conn).await?;
    let s_uid = maybe_user.as_ref().map(|u| &*u.uid);
    let privi = if let Some(uid) = s_uid {
        privilege::check(uid, "Lean4OJ.ManageProblem", &mut conn).await?
    } else {
        false
    };

    let extend = |mut sql: String, mut args: SmallVec<[&'static (dyn ToSql + Sync); 8]>| -> (String, SmallVec<[&'static (dyn ToSql + Sync); 8]>) {
        match problem_id {
            None => sql.push_str(" where pid is null"),
            Some(0) => sql.push_str(" where pid is not null"),
            Some(ref pid) => {
                let _ = write!(&mut sql, " where pid = ${}", args.len() + 1);
                args.push(
                    unsafe { core::mem::transmute::<&i32, &'static i32>(pid) } as _
                );
            }
        }
        if has_kw {
            let _ = write!(&mut sql, " and title ilike ${}", args.len() + 1);
            args.push(
                unsafe { core::mem::transmute::<&&str, &'static &str>(&ekw) } as _
            );
        }
        if has_uid {
            let _ = write!(&mut sql, " and publisher = ${}", args.len() + 1);
            args.push(
                unsafe { core::mem::transmute::<&&str, &'static &str>(&uid) } as _
            );
        }
        if problem_id.is_some() && !privi {
            if let Some(ref uid) = s_uid {
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

    if title_only == Some(true) {
        let discussions = if kw.contains(Discussion::MAGIC_PREFIX) {
            Vec::new()
        } else {
            let mut discussions = Discussion::search(skip_count, take_count, extend, &mut conn).await?;
            for d in &mut discussions { d.backdoor(locale.as_deref()); }
            #[allow(clippy::transmute_undefined_repr)]
            unsafe { core::mem::transmute::<Vec<Discussion>, Vec<Inner1>>(discussions) }
        };
        serde_json::to_writer(unsafe { res.as_mut_vec() }, &discussions)?;
        let count = Discussion::count_aoe(extend, &mut conn).await?;
        write!(&mut res, r#","count":{count}}}"#)?;
    } else {
        let mut discussions = Vec::new();
        if !kw.contains(Discussion::MAGIC_PREFIX) {
            discussions = Discussion::search_aoe(skip_count, take_count, extend, &mut conn)
                .await?
                .into_iter()
                .map(|(meta, problem, publisher)| Inner3 {
                    meta,
                    problem: problem.map(|problem| Inner2 {
                        problem,
                        locale: locale.as_deref(),
                    }),
                    publisher,
                })
                .collect::<Vec<Inner3>>();
            for d in &mut discussions { d.meta.backdoor(locale.as_deref()); }
        }
        serde_json::to_writer(unsafe { res.as_mut_vec() }, &discussions)?;
        let count = Discussion::count_aoe(extend, &mut conn).await?;
        write!(&mut res, r#","permissions":{{"createDiscussion":true,"filterNonpublic":true}},"count":{count}"#)?;
        if has_uid {
            res.push_str(r#","filterPublisher":"#);
            let user_slot: Option<User>;
            let user_ref: &User =
                if let Some(Inner3 { publisher, .. }) = discussions.first() {
                    publisher
                } else {
                    user_slot = User::by_uid(uid, &mut conn).await?;
                    if let Some(u) = user_slot.as_ref() { u } else { return NO_SUCH_USER; }
                };
            serde_json::to_writer(unsafe { res.as_mut_vec() }, user_ref)?;
        }
        if let Some(pid) = problem_id && pid != 0 {
            res.push_str(r#","filterProblem":"#);
            let problem_slot: Option<Inner2>;
            let problem_ref: &Option<Inner2> =
                if let Some(Inner3 { problem, .. }) = discussions.first() {
                    problem
                } else {
                    problem_slot = if privi {
                        Problem::by_pid(pid, &mut conn).await
                    } else {
                        Problem::by_pid_uid(pid, s_uid.unwrap_or_default(), &mut conn).await
                    }?.map(|problem| Inner2 {
                        problem, locale: locale.as_deref()
                    });
                    &problem_slot
                };
            serde_json::to_writer(unsafe { res.as_mut_vec() }, problem_ref)?;
        }
        res.push('}');
    }
    JkmxJsonResponse::Response(StatusCode::OK, res.into())
}

const fn get_discussion_permissions(header: &'static Parts) -> RawPayload {
    RawPayload { header, body: br#"{"permissions":{"userPermissions":[],"groupPermissions":[]},"haveManagePermissionsPermission":true}"# }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetDiscussionRequest {
    locale: Option<CompactString>,
    discussion_id: u32,
    #[serde(flatten)]
    query_replies_type: Option<QueryRepliesType>,
    get_discussion: Option<bool>,
}

#[derive(Serialize)]
struct Inner4<'a> {
    meta: Discussion,
    content: CompactString,
    problem: Option<Inner2<'a>>,
    publisher: UserAOE,
    reactions: DiscussionReactionAOE,
    permissions: [&'static str; 5],
}

const PERMISSION_DEFAULT: [&str; 5] = ["View", "Modify", "ManagePermission", "ManagePublicness", "Delete"];

struct Inner5<'a> {
    replies: &'a [DiscussionReply],
    lookup: &'a HashMap<CompactString, UserAOE>,
    lookup2: &'a HashMap<i32, DiscussionReactionAOE>,
}

impl Serialize for Inner5<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer
    {
        let mut seq = serializer.serialize_seq(Some(self.replies.len()))?;
        for reply in self.replies {
            seq.serialize_element(&DiscussionReplyAOE {
                reply,
                publisher: self.lookup.get(&reply.publisher),
                reactions: self.lookup2.get(&(!reply.id).cast_signed()),
                permissions: REPLY_PERMISSION_DEFAULT,
            })?;
        }
        seq.end()
    }
}

struct Inner6 {
    replies: Vec<DiscussionReply>,
    lookup: HashMap<CompactString, UserAOE>,
    lookup2: HashMap<i32, DiscussionReactionAOE>,
    count: u64,
    split_at: usize,
}

impl Serialize for Inner6 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer
    {
        let mut map = serializer.serialize_map(None)?;
        if self.split_at == usize::MAX {
            map.serialize_entry("repliesInRange", &Inner5 {
                replies: &self.replies,
                lookup: &self.lookup,
                lookup2: &self.lookup2,
            })?;
            map.serialize_entry("repliesCountInRange", &self.count)?;
        } else {
            map.serialize_entry("repliesHead", &Inner5 {
                replies: unsafe { self.replies.get_unchecked(..self.split_at) },
                lookup: &self.lookup,
                lookup2: &self.lookup2,
            })?;
            map.serialize_entry("repliesTail", &Inner5 {
                replies: unsafe { self.replies.get_unchecked(self.split_at..) },
                lookup: &self.lookup,
                lookup2: &self.lookup2,
            })?;
            map.serialize_entry("repliesInRange", &self.split_at)?;
        }
        map.end()
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GetDiscussionResponse<'a> {
    discussion: Option<Inner4<'a>>,
    #[serde(flatten)]
    replies: Option<Inner6>,
    permission_create_new_discussion: bool,
}

async fn get_discussion(
    Session_(session): Session_,
    req: JsonReqult<GetDiscussionRequest>,
) -> JkmxJsonResponse {
    let Json(GetDiscussionRequest { locale, discussion_id, query_replies_type, get_discussion }) = req?;

    let mut res = GetDiscussionResponse {
        discussion: None,
        replies: None,
        permission_create_new_discussion: true,
    };

    let 𝑘 = if get_discussion == Some(true) {
        Some(discussion_id.cast_signed())
    } else {
        None
    };

    let mut conn = get_connection().await?;
    let maybe_user = User::from_maybe_session(&session, &mut conn).await?;
    let uid = maybe_user.as_ref().map(|u| &*u.uid);

    match query_replies_type {
        Some(QueryRepliesType::HeadTail { head_take_count, tail_take_count }) => {
            let head = head_take_count.min(50);
            let tail = tail_take_count.min(50);
            let replies = DiscussionReply::stat_head_tail(discussion_id, head, tail, &mut conn).await?;
            let lookup = privilege::get_area_of_effect(replies.iter().map(|r| &*r.publisher), &mut conn).await?;
            let lookup2 = reaction_aoe(replies.iter().map(|r| (!r.id).cast_signed()).chain(𝑘), uid, &mut conn).await?;
            res.replies = Some(Inner6 {
                lookup,
                lookup2,
                count: replies.len() as u64,
                split_at: replies.len().min(head as usize),
                replies,
            });
        }
        Some(QueryRepliesType::IdRange { before_id, after_id, id_range_take_count }) => {
            let count = id_range_take_count.min(100);
            let replies = DiscussionReply::stat_interval(discussion_id, before_id, after_id, count, &mut conn).await?;
            let lookup = privilege::get_area_of_effect(replies.iter().map(|r| &*r.publisher), &mut conn).await?;
            let lookup2 = reaction_aoe(replies.iter().map(|r| (!r.id).cast_signed()).chain(𝑘), uid, &mut conn).await?;
            res.replies = Some(Inner6 {
                lookup,
                lookup2,
                count: replies.len() as u64,
                split_at: usize::MAX,
                replies,
            });
        }
        None => (),
    }

    if let Some(did) = 𝑘 {
        let Some((mut discussion, publisher)) = Discussion::by_id_aoe(did.cast_unsigned(), &mut conn).await? else { return NO_SUCH_DISCUSSION };
        discussion.backdoor(locale.as_deref());
        let content = mem::take(&mut discussion.content);
        let privi = privilege::all(&publisher.uid, &mut conn).await?;

        // count
        if let Some(Inner6 { split_at: 0..usize::MAX, ref mut count, .. }) = res.replies {
            *count = discussion.reply_count.into();
        }

        // reaction
        let reactions = if let Some(Inner6 { ref mut lookup2, .. }) = res.replies {
            lookup2.remove(&did)
        } else {
            reaction_aoe(𝑘.into_iter(), uid, &mut conn).await?.remove(&did)
        }.unwrap_or_default();

        let problem = if let Some(pid) = discussion.problem_id {
            if let Some(uid) = uid && privilege::check(uid, "Lean4OJ.ManageProblem", &mut conn).await? {
                Problem::by_pid(pid.get(), &mut conn).await
            } else {
                Problem::by_pid_uid(pid.get(), uid.unwrap_or_default(), &mut conn).await
            }?
        } else {
            None
        };
        res.discussion = Some(Inner4 {
            meta: discussion,
            content,
            problem: problem.map(|problem| Inner2 {
                problem,
                locale: locale.as_deref(),
            }),
            publisher: UserAOE {
                user: publisher,
                is_admin: privilege::is_admin(&privi),
                is_problem_admin: privi.iter().any(|p| p == "ManageProblem"),
                is_contest_admin: privi.iter().any(|p| p == "ManageContest"),
                is_discussion_admin: privi.iter().any(|p| p == "ManageDiscussion"),
            },
            reactions,
            permissions: PERMISSION_DEFAULT,
        });
    }

    JkmxJsonResponse::Response(StatusCode::OK, serde_json::to_vec(&res)?.into())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateDiscussionRequest {
    discussion_id: u32,
    title: CompactString,
    content: CompactString,
}

async fn update_discussion(
    Extension(now): Extension<SystemTime>,
    Session_(session): Session_,
    req: JsonReqult<UpdateDiscussionRequest>,
) -> JkmxJsonResponse {
    const SQL_PRIV: &str = "update lean4oj.discussions set title = $1, content = $2, edit = $3, update = $3 where id = $4";
    const SQL: &str = "update lean4oj.discussions set title = $1, content = $2, edit = $3 where id = $4 and publisher = $5";

    let Json(UpdateDiscussionRequest { discussion_id, title, content }) = req?;

    let mut conn = get_connection().await?;
    exs!(user, &session, &mut conn);

    let n = if privilege::check(&user.uid, "Lean4OJ.ManageDiscussion", &mut conn).await? {
        let stmt = conn.prepare_static(SQL_PRIV.into()).await?;
        conn.execute(&stmt, &[&&*title, &&*content, &now, &discussion_id.cast_signed()]).await
    } else {
        let stmt = conn.prepare_static(SQL.into()).await?;
        conn.execute(&stmt, &[&&*title, &&*content, &now, &discussion_id.cast_signed(), &&*user.uid]).await
    }?;
    if n != 1 { return private::err(); }

    JkmxJsonResponse::Response(StatusCode::OK, BYTES_EMPTY)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateReplyRequest {
    discussion_reply_id: u32,
    content: CompactString,
}

async fn update_reply(
    Extension(now): Extension<SystemTime>,
    Session_(session): Session_,
    req: JsonReqult<UpdateReplyRequest>,
) -> JkmxJsonResponse {
    const SQL_PRIV: &str = "update lean4oj.discussion_replies set content = $1, edit = $2 where id = $3 returning did";
    const SQL: &str = "update lean4oj.discussion_replies set content = $1, edit = $2 where id = $3 and publisher = $4 returning did";
    const SQL_UPDATE_PARENT: &str = "update lean4oj.discussions set update = $1 where id = $2";

    let Json(UpdateReplyRequest { discussion_reply_id, content }) = req?;

    let mut conn = get_connection().await?;
    exs!(user, &session, &mut conn);

    let row = if privilege::check(&user.uid, "Lean4OJ.ManageDiscussion", &mut conn).await? {
        let stmt = conn.prepare_static(SQL_PRIV.into()).await?;
        conn.query_one(&stmt, &[&&*content, &now, &discussion_reply_id.cast_signed()]).await?
    } else {
        let stmt = conn.prepare_static(SQL.into()).await?;
        conn.query_one(&stmt, &[&&*content, &now, &discussion_reply_id.cast_signed(), &&*user.uid]).await?
    };
    let did = row.try_get::<_, i32>(0)?;
    let stmt = conn.prepare_static(SQL_UPDATE_PARENT.into()).await?;
    let n = conn.execute(&stmt, &[&now, &did]).await?;
    if n != 1 { return private::err(); }

    let res = format!(r#"{{"editTime":{}}}"#, get_millis(now));
    JkmxJsonResponse::Response(StatusCode::OK, res.into())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeleteDiscussionRequest {
    discussion_id: u32,
}

async fn delete_discussion(
    Session_(session): Session_,
    req: JsonReqult<DeleteDiscussionRequest>,
) -> JkmxJsonResponse {
    const SQL_PRIV: &str = "delete from lean4oj.discussions where id = $1";
    const SQL: &str = "delete from lean4oj.discussions where id = $1 and publisher = $2";

    let Json(DeleteDiscussionRequest { discussion_id }) = req?;

    let mut conn = get_connection().await?;
    exs!(user, &session, &mut conn);

    let n = if privilege::check(&user.uid, "Lean4OJ.ManageDiscussion", &mut conn).await? {
        let stmt = conn.prepare_static(SQL_PRIV.into()).await?;
        conn.execute(&stmt, &[&discussion_id.cast_signed()]).await?
    } else {
        let stmt = conn.prepare_static(SQL.into()).await?;
        conn.execute(&stmt, &[&discussion_id.cast_signed(), &&*user.uid]).await?
    };
    if n != 1 { return private::err(); }

    JkmxJsonResponse::Response(StatusCode::OK, BYTES_EMPTY)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeleteReplyRequest {
    discussion_reply_id: u32,
}

async fn delete_reply(
    Session_(session): Session_,
    req: JsonReqult<DeleteReplyRequest>,
) -> JkmxJsonResponse {
    const SQL_PRIV: &str = "delete from lean4oj.discussion_replies where id = $1 returning did";
    const SQL: &str = "delete from lean4oj.discussion_replies where id = $1 and publisher = $2 returning did";
    const SQL_UPDATE_PARENT: &str = "update lean4oj.discussions set update = greatest(discussions.edit, (select max(discussion_replies.edit) from lean4oj.discussion_replies where did = $1)), reply_count = reply_count - 1 where id = $1";

    let Json(DeleteReplyRequest { discussion_reply_id }) = req?;

    let mut conn = get_connection().await?;
    exs!(user, &session, &mut conn);

    let txn;
    let stmt_p = conn.prepare_static(SQL_UPDATE_PARENT.into()).await?;
    let row = if privilege::check(&user.uid, "Lean4OJ.ManageDiscussion", &mut conn).await? {
        let stmt = conn.prepare_static(SQL_PRIV.into()).await?;
        txn = conn.transaction().await?;
        txn.query_one(&stmt, &[&discussion_reply_id.cast_signed()]).await?
    } else {
        let stmt = conn.prepare_static(SQL.into()).await?;
        txn = conn.transaction().await?;
        txn.query_one(&stmt, &[&discussion_reply_id.cast_signed(), &&*user.uid]).await?
    };
    let did = row.try_get::<_, i32>(0)?;
    let n = txn.execute(&stmt_p, &[&did]).await?;
    if n != 1 { return private::err(); }
    txn.commit().await?;

    JkmxJsonResponse::Response(StatusCode::OK, BYTES_EMPTY)
}

pub fn router(header: &'static Parts) -> Router {
    Router::new()
        .route("/createDiscussion", post(create_discussion))
        .route("/createDiscussionReply", post(create_reply))
        .route("/toggleReaction", post(reaction))
        .route("/queryDiscussion", post(query_discussions))
        .route("/getDiscussionPermissions", post_service(get_discussion_permissions(header)))
        .route("/getDiscussionAndReplies", post(get_discussion))
        .route("/updateDiscussion", post(update_discussion))
        .route("/updateDiscussionReply", post(update_reply))
        .route("/deleteDiscussion", post(delete_discussion))
        .route("/deleteDiscussionReply", post(delete_reply))
}
