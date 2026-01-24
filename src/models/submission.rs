use core::{
    fmt,
    future::ready,
    pin::Pin,
    ptr,
    task::{Context, Poll},
};
use std::{sync::LazyLock, time::SystemTime};

use axum::response::sse::Event;
use compact_str::CompactString;
use dashmap::{DashMap, Entry};
use futures_util::{Stream, TryStreamExt};
use hashbrown::{DefaultHashBuilder, HashMap};
use serde::{Serialize, ser::SerializeMap};
use smallvec::{SmallVec, smallvec};
use tokio::sync::broadcast;
use tokio_postgres::{Client, Row, types::ToSql};

mod aoe;
pub use aoe::Aoe as SubmissionAoe;
mod message;
pub use message::Action as SubmissionMessageAction;
mod status;
pub use status::Status as SubmissionStatus;

use crate::{
    libs::{
        db::{DBError, DBResult, ToSqlIter},
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
        let status = row.try_get("status")?;
        let message = row.try_get::<_, &str>("message")?.into();
        let answer_size = row.try_get::<_, i64>("answer_size")?.cast_unsigned();
        let answer_hash = row.try_get::<_, &[u8]>("answer_hash")?;
        let answer_hash = answer_hash.try_into().map_err(|e|
            DBError::new(tokio_postgres::error::Kind::FromSql(10), Some(Box::new(e)))
        )?;
        let answer_obj = row.try_get::<_, &str>("answer_obj")?.into();
        Ok(Self { sid, pid, submitter, submit_time, module_name, const_name, lean_toolchain, status, message, answer_size, answer_hash, answer_obj })
    }
}

#[inline]
fn 𝓈(row: Row) -> DBResult<(Submission, User)> {
    Ok((row.clone().try_into()?, row.try_into()?))
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
        const SQL: &str = "update lean4oj.submissions set status = $1 where sid = $2 returning old.status, pid, submitter";
        const SQL_REPLACE: &str = "update lean4oj.submissions set status = $1, message = $2 where sid = $3 returning old.status, pid, submitter";
        const SQL_APPEND: &str = "update lean4oj.submissions set status = $1, message = message || $2 where sid = $3 returning old.status, pid, submitter";
        const SQL_PROBLEM_AC: &str = "update lean4oj.problems set pac = pac + $1 where pid = $2";
        const SQL_USER_AC: &str = "update lean4oj.users set ac = (select count(distinct pid) from lean4oj.submissions where submitter = $1 and status = '\x09') where uid = $1";

        let row = match msg {
            SubmissionMessageAction::NoAction => {
                let stmt = db.prepare_static(SQL.into()).await?;
                db.query_one(&stmt, &[&(status as u8).cast_signed(), &sid.cast_signed()]).await
            }
            SubmissionMessageAction::Replace(ref m) => {
                let stmt = db.prepare_static(SQL_REPLACE.into()).await?;
                db.query_one(&stmt, &[&(status as u8).cast_signed(), &m, &sid.cast_signed()]).await
            }
            SubmissionMessageAction::Append(ref m) => {
                let stmt = db.prepare_static(SQL_APPEND.into()).await?;
                db.query_one(&stmt, &[&(status as u8).cast_signed(), &m, &sid.cast_signed()]).await
            }
        }?;
        let old = row.try_get::<_, SubmissionStatus>(0)?;

        let delta = i32::from(status == SubmissionStatus::Accepted) - i32::from(old == SubmissionStatus::Accepted);
        if delta != 0 {
            let pid = row.try_get::<_, i32>(1)?;
            let submitter = row.try_get::<_, &str>(2)?;
            let stmt_problem_ac = db.prepare_static(SQL_PROBLEM_AC.into()).await?;
            db.execute(&stmt_problem_ac, &[&delta, &pid]).await?;
            let stmt_user_ac = db.prepare_static(SQL_USER_AC.into()).await?;
            db.execute(&stmt_user_ac, &[&submitter]).await?;
        }

        if let Some(tx) = FOOD.get(&sid) {
            let _ = tx.send(UserUpdate::Status(status, msg));
        }

        Ok(())
    }

    pub async fn report_answer(sid: u32, answer: CompactString, db: &mut Client) -> DBResult<()> {
        const SQL: &str = "update lean4oj.submissions set answer_obj = $1 where sid = $2";

        let stmt = db.prepare_static(SQL.into()).await?;
        let n = db.execute(&stmt, &[&&*answer, &sid.cast_signed()]).await?;
        if n != 1 {
            return Err(DBError::new(tokio_postgres::error::Kind::RowCount, Some("answer update error".into())));
        }

        if let Some(tx) = FOOD.get(&sid) {
            let _ = tx.send(UserUpdate::Answer(answer));
        }

        Ok(())
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

    pub async fn stat_aoe(pid: i32, skip: i64, take: i64, db: &mut Client) -> DBResult<Vec<(Self, User)>> {
        const SQL: &str = "select sid, pid, submitter, submit_time, module_name, const_name, lean_toolchain, status, message, answer_size, answer_hash, answer_obj, uid, password, username, email, register_time, ac, nickname, bio, avatar_info from lean4oj.submissions inner join lean4oj.users on submitter = uid where pid = $1 and status = '\x09' order by sid offset $2 limit $3";

        let stmt = db.prepare_static(SQL.into()).await?;
        let params: [&(dyn ToSql + Sync); 3] = [&pid, &skip, &take];
        let stream = db.query_raw(&stmt, params).await?;
        stream.and_then(|row| ready(𝓈(row))).try_collect().await
    }

    pub async fn stat_count(pid: i32, db: &mut Client) -> DBResult<[u64; 2]> {
        const SQL: &str = "select status = '\x09', count(*) from lean4oj.submissions where pid = $1 group by status = '\x09'";

        let stmt = db.prepare_static(SQL.into()).await?;
        let stream = db.query_raw(&stmt, [pid]).await?;
        let mut ret = [0; 2];
        stream.try_for_each(|row| ready(try {
            let ac: bool = row.try_get(0)?;
            let count: i64 = row.try_get(1)?;
            ret[usize::from(ac)] = count.cast_unsigned();
        })).await?;
        Ok(ret)
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

    pub async fn by_uid_pids<I>(uid: &str, pids: I, db: &mut Client) -> DBResult<HashMap<i32, (u32, SubmissionStatus)>>
    where
        I: ExactSizeIterator<Item = i32> + Clone + fmt::Debug + Sync,
    {
        const SQL: &str = "select distinct on (pid, status = '\x09') sid, pid, status from lean4oj.submissions where submitter = $1 and pid = any($2) order by pid, status = '\x09', sid desc";

        let mut lookup = HashMap::with_capacity(pids.len());

        let stmt = db.prepare_static(SQL.into()).await?;
        let params: [&(dyn ToSql + Sync); 2] = [&uid, &ToSqlIter(pids)];
        let stream = db.query_raw(&stmt, params).await?;

        stream.try_for_each(|row| ready(try {
            let sid = row.try_get::<_, i32>(0)?.cast_unsigned();
            let pid = row.try_get(1)?;
            let status = row.try_get(2)?;
            lookup.insert(pid, (sid, status));
        })).await?;
        Ok(lookup)
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
        map.serialize_entry("leanVersion", &format_args!("4{}", self.submission.lean_toolchain))?;
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


#[derive(Clone, Serialize)]
enum UserUpdate {
    Status(SubmissionStatus, SubmissionMessageAction),
    Answer(CompactString),
}

static FOOD: LazyLock<
    DashMap<u32, broadcast::Sender<UserUpdate>, DefaultHashBuilder>
> = LazyLock::new(|| DashMap::with_hasher(DefaultHashBuilder::default()));

#[repr(transparent)]
pub struct UserSubscription {
    #[allow(clippy::type_complexity)]
    inner: SmallVec<[
        (u32, broadcast::Receiver<UserUpdate>, Option<broadcast::Recv<'static, UserUpdate>>);
        Self::MAX_SUBSCRIPTION
    ]>,
}

impl UserSubscription {
    pub const MAX_SUBSCRIPTION: usize = 10;

    async fn wait(sid: u32, tx: broadcast::Sender<UserUpdate>) {
        let addr = unsafe { *(&raw const tx).cast::<usize>() };
        tx.closed().await;
        FOOD.remove_if(&sid, |_, tx1| {
            let ret = unsafe { *ptr::from_ref(tx1).cast::<usize>() } == addr;
            #[cfg(debug_assertions)]
            if ret {
                tracing::info!("UserSubscription for sid #{sid} removed.");
            }
            ret
        });
    }

    fn make_one(sid: u32) -> broadcast::Receiver<UserUpdate> {
        match FOOD.entry(sid) {
            Entry::Occupied(e) => e.get().subscribe(),
            Entry::Vacant(e) => {
                let (tx, rx) = broadcast::channel(16);
                e.insert(tx.clone());
                tokio::spawn(Self::wait(sid, tx));
                rx
            }
        }
    }

    pub fn new(ids: &[u32]) -> Self {
        debug_assert!(ids.len() <= Self::MAX_SUBSCRIPTION);
        Self {
            inner: ids.iter()
                .map(|&sid| (sid, Self::make_one(sid), None))
                .collect(),
        }
    }
}

impl Stream for UserSubscription {
    type Item = Result<Event, broadcast::error::RecvError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut buf = fmt::NumBuffer::new();
        let inner = &mut *unsafe { self.get_unchecked_mut() }.inner;
        for &mut (sid, ref mut rx, ref mut waiter) in inner {
            let recv = waiter.get_or_insert_with(|| broadcast::Recv::new(unsafe { &mut *ptr::from_mut(rx) }));
            match rx.recv_ref(Some((recv.inner(), cx.waker()))) {
                Ok(value) => {
                    *waiter = None;
                    let Some(update) = value.value() else { return Poll::Ready(Some(Err(broadcast::error::RecvError::Closed))) };

                    let event = Event::default()
                        .id(sid.format_into(&mut buf))
                        .event("update")
                        .json_data(update)
                        .unwrap();
                    return Poll::Ready(Some(Ok(event)));
                }
                Err(broadcast::error::TryRecvError::Empty) => (),
                Err(broadcast::error::TryRecvError::Closed) => {
                    *waiter = None;
                    return Poll::Ready(Some(Err(broadcast::error::RecvError::Closed)));
                }
                Err(broadcast::error::TryRecvError::Lagged(n)) => {
                    *waiter = None;
                    return Poll::Ready(Some(Err(broadcast::error::RecvError::Lagged(n))));
                }
            }
        }

        Poll::Pending
    }
}
