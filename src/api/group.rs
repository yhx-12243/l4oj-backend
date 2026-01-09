use axum::{
    Json, Router,
    routing::{get, post},
};
use compact_str::CompactString;
use http::{StatusCode, response::Parts};
use serde::{Deserialize, Serializer, ser::SerializeSeq};
use serde_json::ser::Serializer as JSerializer;
use tokio_postgres::Client;

use crate::{
    bad, exs,
    libs::{
        auth::Session_,
        constants::{BYTES_EMPTY, BYTES_NULL},
        db::{DBError, DBResult, get_connection},
        request::JsonReqult,
        response::JkmxJsonResponse,
        serde::WithJson,
        validate::{check_groupname, is_admin_group, is_system_group},
    }, models::{
        group::{AUV, Group, GroupA},
        user::User,
    }
};

mod private {
    pub(super) fn err() -> super::JkmxJsonResponse {
        let err = super::DBError::new(tokio_postgres::error::Kind::RowCount, Some("database group error".into()));
        return super::JkmxJsonResponse::Error(super::StatusCode::INTERNAL_SERVER_ERROR, err.into());
    }

    #[inline]
    pub(super) async fn μ(
        uid: &str,
        gid: &str,
        赛博灯泡: bool, // Attempt to change self.
        conn: &mut super::Client,
    ) -> super::DBResult<bool> {
        const SQL_ADMIN: &str = "select 1 from lean4oj.user_groups where uid = $1 and gid = 'Lean4OJ.Admin' and is_admin = true limit 1";
        // For admin groups, `Admin` privilege is not sufficient, must `Admin` + `is_admin`.
        const SQL_SYSTEM: &str = "select 1 from lean4oj.user_groups where uid = $1 and (gid = 'Lean4OJ.Admin' or (gid = $2 and is_admin = true)) limit 1";
        // For system groups, `ManageUserGroup` privilege is not sufficient.
        const SQL_NORMAL: &str = "select 1 from lean4oj.user_groups where uid = $1 and (gid = 'Lean4OJ.ManageUserGroup' or gid = 'Lean4OJ.Admin' or (gid = $2 and is_admin = true)) limit 1";
        const SQL_SYSTEM_PROPER: &str = "select 1 from lean4oj.user_groups where uid = $1 and gid = 'Lean4OJ.Admin' limit 1";
        const SQL_NORMAL_PROPER: &str = "select 1 from lean4oj.user_groups where uid = $1 and (gid = 'Lean4OJ.ManageUserGroup' or gid = 'Lean4OJ.Admin') limit 1";

        let (sql, count) =
            if super::is_admin_group(gid) {
                if 赛博灯泡 { return Ok(false) } (SQL_ADMIN, 1)
            } else if super::is_system_group(gid) {
                if 赛博灯泡 { (SQL_SYSTEM_PROPER, 1) } else { (SQL_SYSTEM, 2) }
            } else {
                if 赛博灯泡 { (SQL_NORMAL_PROPER, 1) } else { (SQL_NORMAL, 2) }
            };
        let stmt = conn.prepare_static(sql.into()).await?;
        let buf: [&(dyn tokio_postgres::types::ToSql + Sync); 2] = [&uid, &gid];
        conn.query_opt(
            &stmt, unsafe { buf.get_unchecked(..count) },
        ).await.map(|x| x.is_some())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateGroupRequest {
    group_name: CompactString,
}

async fn create_group(
    Session_(session): Session_,
    req: JsonReqult<CreateGroupRequest>,
) -> JkmxJsonResponse {
    const SQL_CREATE_GROUP: &str = "insert into lean4oj.groups (gid, member_count) values ($1, 1)";
    const SQL_LINK: &str = "insert into lean4oj.user_groups (uid, gid, is_admin) values ($1, $2, true)";

    let Json(CreateGroupRequest { group_name }) = req?;

    if !check_groupname(&group_name) { bad!(BYTES_NULL) }

    let mut conn = get_connection().await?;
    exs!(user, &session, &mut conn);
    // if !privilege::check(&user.uid, "Lean4OJ.ManageUserGroup", &mut conn).await? { return JkmxJsonResponse::Response(StatusCode::FORBIDDEN, BYTES_NULL); }

    let stmt_create = conn.prepare_static(SQL_CREATE_GROUP.into()).await?;
    let stmt_link = conn.prepare_static(SQL_LINK.into()).await?;
    let txn = conn.transaction().await?;
    let n = txn.execute(&stmt_create, &[&&*group_name]).await?;
    if n != 1 { return private::err(); }
    let n = txn.execute(&stmt_link, &[&&*user.uid, &&*group_name]).await?;
    if n != 1 { return private::err(); }
    txn.commit().await?;

    let res = format!(r#"{{"groupId":"{group_name}"}}"#);
    JkmxJsonResponse::Response(StatusCode::OK, res.into())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UidGidRequest {
    user_id: CompactString,
    group_id: CompactString,
}

async fn add_member(
    Session_(session): Session_,
    req: JsonReqult<UidGidRequest>,
) -> JkmxJsonResponse {
    const SQL_ADD: &str = "insert into lean4oj.user_groups (uid, gid) values ($1, $2)";
    const SQL_UPDATE: &str = "update lean4oj.groups set member_count = member_count + 1 where gid = $1";

    let Json(UidGidRequest { user_id, group_id }) = req?;

    let mut conn = get_connection().await?;
    exs!(s_user, &session, &mut conn);
    if !private::μ(&s_user.uid, &group_id, false, &mut conn).await? { return JkmxJsonResponse::Response(StatusCode::FORBIDDEN, BYTES_NULL); }

    let stmt_add = conn.prepare_static(SQL_ADD.into()).await?;
    let stmt_update = conn.prepare_static(SQL_UPDATE.into()).await?;
    let txn = conn.transaction().await?;
    let n = txn.execute(&stmt_add, &[&&*user_id, &&*group_id]).await?;
    if n != 1 { return private::err(); }
    let n = txn.execute(&stmt_update, &[&&*group_id]).await?;
    if n != 1 { return private::err(); }
    txn.commit().await?;

    JkmxJsonResponse::Response(StatusCode::OK, BYTES_EMPTY)
}

async fn remove_member(
    Session_(session): Session_,
    req: JsonReqult<UidGidRequest>,
) -> JkmxJsonResponse {
    const SQL_REMOVE: &str = "delete from lean4oj.user_groups where uid = $1 and gid = $2 and is_admin = false";
    const SQL_UPDATE: &str = "update lean4oj.groups set member_count = member_count - 1 where gid = $1";

    let Json(UidGidRequest { user_id, group_id }) = req?;

    let mut conn = get_connection().await?;
    exs!(s_user, &session, &mut conn);
    if !private::μ(&s_user.uid, &group_id, false, &mut conn).await? { return JkmxJsonResponse::Response(StatusCode::FORBIDDEN, BYTES_NULL); }

    let stmt_remove = conn.prepare_static(SQL_REMOVE.into()).await?;
    let stmt_update = conn.prepare_static(SQL_UPDATE.into()).await?;
    let txn = conn.transaction().await?;
    let n = txn.execute(&stmt_remove, &[&&*user_id, &&*group_id]).await?;
    if n != 1 { return private::err(); }
    let n = txn.execute(&stmt_update, &[&&*group_id]).await?;
    if n != 1 { return private::err(); }
    txn.commit().await?;

    JkmxJsonResponse::Response(StatusCode::OK, BYTES_EMPTY)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetGroupAdminRequest {
    user_id: CompactString,
    group_id: CompactString,
    is_group_admin: bool,
}

async fn set_group_admin(
    Session_(session): Session_,
    req: JsonReqult<SetGroupAdminRequest>,
) -> JkmxJsonResponse {
    const SQL_SET_ADMIN: &str = "update lean4oj.user_groups set is_admin = $1 where uid = $2 and gid = $3";

    let Json(SetGroupAdminRequest { user_id, group_id, is_group_admin }) = req?;

    let mut conn = get_connection().await?;
    exs!(s_user, &session, &mut conn);
    let Some(t_user) = User::by_uid(&user_id, &mut conn).await? else { return JkmxJsonResponse::Response(StatusCode::NOT_FOUND, BYTES_NULL) };
    if !private::μ(&s_user.uid, &group_id, *s_user.uid == *t_user.uid, &mut conn).await? { return JkmxJsonResponse::Response(StatusCode::FORBIDDEN, BYTES_NULL); }

    let stmt_set_admin = conn.prepare_static(SQL_SET_ADMIN.into()).await?;
    let n = conn.execute(&stmt_set_admin, &[&is_group_admin, &&*user_id, &&*group_id]).await?;
    if n != 1 { return private::err(); }

    JkmxJsonResponse::Response(StatusCode::OK, BYTES_EMPTY)
}

async fn get_group_list(Session_(session): Session_) -> JkmxJsonResponse {
    let mut conn = get_connection().await?;
    let res = if let Some(user) = User::from_maybe_session(&session, &mut conn).await? {
        let groups = GroupA::list(&user.uid, &mut conn).await?;
        let mut buf = format!(r#"{{"groups":{},"groupsWithAdminPermission":"#, WithJson(&*groups));
        let mut ser = JSerializer::new(unsafe { buf.as_mut_vec() });
        let mut seq = ser.serialize_seq(None)?;
        for &GroupA { ref group, is_admin} in &groups {
            if is_admin {
                seq.serialize_element(&group.gid)?;
            }
        }
        seq.end()?;
        buf.push('}');
        buf
    } else {
        let groups = Group::list(&mut conn).await?;
        format!(r#"{{"groups":{},"groupsWithAdminPermission":[]}}"#, WithJson(groups))
    };
    JkmxJsonResponse::Response(StatusCode::OK, res.into())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetGroupMemberListRequest {
    group_id: CompactString,
}

async fn get_group_member_list(req: JsonReqult<GetGroupMemberListRequest>) -> JkmxJsonResponse {
    let Json(GetGroupMemberListRequest { group_id }) = req?;

    let mut conn = get_connection().await?;
    let members = AUV::list(&group_id, &mut conn).await?;
    let res = format!(r#"{{"memberList":{}}}"#, WithJson(members));
    JkmxJsonResponse::Response(StatusCode::OK, res.into())
}

pub fn router(_header: &'static Parts) -> Router {
    Router::new()
        .route("/createGroup", post(create_group))
        .route("/addMember", post(add_member))
        .route("/removeMember", post(remove_member))
        .route("/setGroupAdmin", post(set_group_admin))
        .route("/getGroupList", get(get_group_list))
        .route("/getGroupMemberList", post(get_group_member_list))
}
