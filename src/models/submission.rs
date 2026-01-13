use core::future::ready;
use std::time::SystemTime;

use compact_str::CompactString;
use futures_util::TryStreamExt;
use serde::{Serialize, ser::SerializeMap};
use smallvec::{SmallVec, smallvec};
use tokio_postgres::{Client, Row, types::ToSql};

mod aoe;
pub use aoe::Aoe as SubmissionAoe;
mod message;
pub use message::Action as SubmissionMessageAction;
mod status;
pub use status::Status as SubmissionStatus;

use crate::{
    libs::{
        db::{DBError, DBResult},
        util::get_millis,
    },
    models::{problem::Problem, user::User},
};

pub struct Submission {
    pub sid: u32,
    pub pid: i32,
    pub submitter: CompactString,
    pub submit_time: SystemTime,
    pub module_name: CompactString,
    pub const_name: CompactString,
    pub lean_toolchain: CompactString,
    pub status: SubmissionStatus,
    pub message: CompactString,
    pub answer_size: u64,
    pub answer_hash: [u8; 32],
    pub answer_obj: CompactString,
}

impl TryFrom<Row> for Submission {
    type Error = DBError;

    fn try_from(row: Row) -> Result<Self, Self::Error> {
        let sid = row.try_get::<_, i32>("sid")?.cast_unsigned();
        let pid = row.try_get("pid")?;
        let submitter = row.try_get::<_, &str>("submitter")?.into();
        let submit_time = row.try_get("submit_time")?;
        let module_name = row.try_get::<_, &str>("module_name")?.into();
        let const_name = row.try_get::<_, &str>("const_name")?.into();
        let lean_toolchain = row.try_get::<_, &str>("lean_toolchain")?.into();
        let status = row.try_get::<_, i8>("status")?;
        let status = status.cast_unsigned().try_into().map_err(|()|
            DBError::new(tokio_postgres::error::Kind::FromSql(7), None)
        )?;
        let message = row.try_get::<_, &str>("message")?.into();
        let answer_size = row.try_get::<_, i64>("answer_size")?.cast_unsigned();
        let answer_hash = row.try_get::<_, &[u8]>("answer_hash")?;
        let answer_hash = answer_hash.try_into().map_err(|e|
            DBError::new(tokio_postgres::error::Kind::FromSql(9), Some(Box::new(e)))
        )?;
        let answer_obj = row.try_get::<_, &str>("answer_obj")?.into();
        Ok(Self { sid, pid, submitter, submit_time, module_name, const_name, lean_toolchain, status, message, answer_size, answer_hash, answer_obj })
    }
}

#[inline]
fn 𝒮(row: Row) -> DBResult<(Submission, Problem, User)> {
    Ok((row.clone().try_into()?, row.clone().try_into()?, row.try_into()?))
}

impl Submission {
    #[allow(clippy::too_many_arguments)]
    pub async fn create(
        pid: i32, submitter: &str, submit_time: SystemTime,
        module_name: &str, const_name: &str, lean_toolchain: &str,
        answer_size: u64, answer_hash: [u8; 32],
        db: &mut Client,
    ) -> DBResult<u32> {
        const SQL: &str = "insert into lean4oj.submissions (pid, submitter, submit_time, module_name, const_name, lean_toolchain, answer_size, answer_hash) values ($1, $2, $3, $4, $5, $6, $7, $8) returning sid";

        let stmt = db.prepare_static(SQL.into()).await?;
        let row = db.query_one(&stmt, &[
            &pid, &submitter, &submit_time,
            &module_name, &const_name, &lean_toolchain,
            &answer_size.cast_signed(), &answer_hash.as_slice(),
        ]).await?;
        row.try_get::<_, i32>(0).map(i32::cast_unsigned)
    }

    pub async fn report_status(sid: u32, status: SubmissionStatus, msg: SubmissionMessageAction, db: &mut Client) -> DBResult<()> {
        const SQL: &str = "update lean4oj.submissions set status = $1 where sid = $2";
        const SQL_REPLACE: &str = "update lean4oj.submissions set status = $1, message = $2 where sid = $3";
        const SQL_APPEND: &str = "update lean4oj.submissions set status = $1, message = message || $2 where sid = $3";

        let n = match msg {
            SubmissionMessageAction::NoAction => {
                let stmt = db.prepare_static(SQL.into()).await?;
                db.execute(&stmt, &[&(status as u8).cast_signed(), &sid.cast_signed()]).await
            }
            SubmissionMessageAction::Replace(m) => {
                let stmt = db.prepare_static(SQL_REPLACE.into()).await?;
                db.execute(&stmt, &[&(status as u8).cast_signed(), &m, &sid.cast_signed()]).await
            }
            SubmissionMessageAction::Append(m) => {
                let stmt = db.prepare_static(SQL_APPEND.into()).await?;
                db.execute(&stmt, &[&(status as u8).cast_signed(), &m, &sid.cast_signed()]).await
            }
        }?;
        if n == 1 {
            Ok(())
        } else {
            Err(DBError::new(tokio_postgres::error::Kind::UnexpectedMessage, None))
        }
    }

    pub async fn by_sid_with_problem(sid: u32, db: &mut Client) -> DBResult<Option<(Self, Problem, User)>> {
        const SQL: &str = "select sid, pid, submitter, submit_time, module_name, const_name, lean_toolchain, status, message, answer_size, answer_hash, answer_obj, is_public, public_at, owner, pcontent, sub, pac, submittable, jb, uid, password, username, email, register_time, ac, nickname, bio, avatar_info from lean4oj.submissions natural join lean4oj.problems inner join lean4oj.users on submitter = uid where sid = $1";

        let stmt = db.prepare_static(SQL.into()).await?;
        let result = match db.query_opt(&stmt, &[&sid.cast_signed()]).await? {
            Some(row) => Some((row.clone().try_into()?, row.clone().try_into()?, row.try_into()?)),
            None => None,
        };
        Ok(result)
    }

    pub async fn by_sid_uid_with_problem(sid: u32, uid: &str, db: &mut Client) -> DBResult<Option<(Self, Problem, User)>> {
        const SQL: &str = "select sid, pid, submitter, submit_time, module_name, const_name, lean_toolchain, status, message, answer_size, answer_hash, answer_obj, is_public, public_at, owner, pcontent, sub, pac, submittable, jb, uid, password, username, email, register_time, ac, nickname, bio, avatar_info from lean4oj.submissions natural join lean4oj.problems inner join lean4oj.users on submitter = uid where sid = $1 and (owner = $2 or is_public)";

        let stmt = db.prepare_static(SQL.into()).await?;
        let result = match db.query_opt(&stmt, &[&sid.cast_signed(), &uid]).await? {
            Some(row) => Some((row.clone().try_into()?, row.clone().try_into()?, row.try_into()?)),
            None => None,
        };
        Ok(result)
    }

    pub async fn search_aoe<'a, F>(take: i64, aoe: aoe::Aoe, extend: F, db: &mut Client) -> DBResult<Vec<(Self, Problem, User)>>
    where
        F: FnOnce(String, SmallVec<[&'a (dyn ToSql + Sync); 8]>) -> (String, SmallVec<[&'a (dyn ToSql + Sync); 8]>),
    {
        let mut sql = "select sid, pid, submitter, submit_time, module_name, const_name, lean_toolchain, status, message, answer_size, answer_hash, answer_obj, is_public, public_at, owner, pcontent, sub, pac, submittable, jb, uid, password, username, email, register_time, ac, nickname, bio, avatar_info from lean4oj.submissions natural join lean4oj.problems inner join lean4oj.users on submitter = uid where ".to_owned();
        let mut args: SmallVec<[&(dyn ToSql + Sync); 8]> = smallvec![
            unsafe { core::mem::transmute::<&i64, &'a i64>(&take) } as _,
        ];
        let trailer = match aoe {
            aoe::Aoe::Global => {
                sql.push_str("true");
                " order by sid desc limit $1"
            }
            aoe::Aoe::After(ref min_id) => {
                sql.push_str("sid >= $2");
                args.push(unsafe { core::mem::transmute::<&u32, &'a i32>(min_id) } as _);
                " order by sid limit $1"
            }
            aoe::Aoe::Before(ref max_id) => {
                sql.push_str("sid <= $2");
                args.push(unsafe { core::mem::transmute::<&u32, &'a i32>(max_id) } as _);
                " order by sid desc limit $1"
            }
        };
        (sql, args) = extend(sql, args);
        sql.push_str(trailer);

        let stmt = db.prepare_static(sql.into()).await?;
        let stream = db.query_raw(&stmt, args).await?;
        stream.and_then(|row| ready(𝒮(row))).try_collect().await
    }

    pub async fn ping_one<'a, F>(aoe: aoe::Aoe, extend: F, db: &mut Client) -> DBResult<bool>
    where
        F: FnOnce(String, SmallVec<[&'a (dyn ToSql + Sync); 8]>) -> (String, SmallVec<[&'a (dyn ToSql + Sync); 8]>),
    {
        let mut sql = "select from lean4oj.submissions natural join lean4oj.problems where ".to_owned();
        let arg: &'a i32 = match aoe {
            aoe::Aoe::Global => unreachable!(),
            aoe::Aoe::After(ref min_id) => {
                sql.push_str("sid < $1");
                unsafe { core::mem::transmute::<&u32, &'a i32>(min_id) }
            }
            aoe::Aoe::Before(ref max_id) => {
                sql.push_str("sid > $1");
                unsafe { core::mem::transmute::<&u32, &'a i32>(max_id) }
            }
        };

        let mut args: SmallVec<[&(dyn ToSql + Sync); 8]> = smallvec![arg as _];
        (sql, args) = extend(sql, args);
        sql.push_str(" limit 1");

        let stmt = db.prepare_static(sql.into()).await?;
        db.query_opt(&stmt, &args).await.map(|row| row.is_some())
    }
}

pub struct SubmissionMeta<'a> {
    pub submission: Submission,
    pub problem: Problem,
    pub submitter: User,
    pub locale: Option<&'a str>
}

impl Serialize for SubmissionMeta<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("id", &self.submission.sid)?;
        map.serialize_entry("isPublic", &self.problem.is_public)?;
        map.serialize_entry("submitTime", &get_millis(self.submission.submit_time))?;
        map.serialize_entry("moduleName", &*self.submission.module_name)?;
        map.serialize_entry("constName", &*self.submission.const_name)?;
        // lean_toolchain
        map.serialize_entry("status", &self.submission.status)?;
        map.serialize_entry("message", &*self.submission.message)?;
        map.serialize_entry("answerSize", &self.submission.answer_size)?;
        // answer_hash
        map.serialize_entry("answerObj", &*self.submission.answer_obj)?;
        map.serialize_entry("problem", &self.problem)?;
        let title = self.problem.content.apply(self.locale).map_or_default(|x| &*x.title);
        map.serialize_entry("problemTitle", title)?;
        map.serialize_entry("submitter", &self.submitter)?;
        map.end()
    }
}
