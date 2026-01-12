use std::time::SystemTime;

use compact_str::CompactString;
use tokio_postgres::{Client, Row};

mod status;
pub use status::Status as SubmissionStatus;

use crate::libs::db::{DBError, DBResult};

pub struct Submission {
    pub sid: u32,
    pub pid: i32,
    pub submitter: CompactString,
    pub time: SystemTime,
    pub module_name: CompactString,
    pub const_name: CompactString,
    pub lean_toolchain: CompactString,
    pub status: SubmissionStatus,
    pub answer_size: u64,
    pub answer_hash: [u8; 32],
}

impl TryFrom<Row> for Submission {
    type Error = DBError;

    fn try_from(row: Row) -> Result<Self, Self::Error> {
        let sid = row.try_get::<_, i32>("sid")?.cast_unsigned();
        let pid = row.try_get("pid")?;
        let submitter = row.try_get::<_, &str>("submitter")?.into();
        let time = row.try_get("time")?;
        let module_name = row.try_get::<_, &str>("module_name")?.into();
        let const_name = row.try_get::<_, &str>("const_name")?.into();
        let lean_toolchain = row.try_get::<_, &str>("lean_toolchain")?.into();
        let status = row.try_get::<_, i8>("status")?;
        let status = status.cast_unsigned().try_into().map_err(|()|
            DBError::new(tokio_postgres::error::Kind::FromSql(7), None)
        )?;
        let answer_size = row.try_get::<_, i64>("answer_size")?.cast_unsigned();
        let answer_hash = row.try_get::<_, &str>("answer_hash")?.as_bytes();
        let answer_hash = answer_hash.try_into().map_err(|e|
            DBError::new(tokio_postgres::error::Kind::FromSql(9), Some(Box::new(e)))
        )?;
        Ok(Submission { sid, pid, submitter, time, module_name, const_name, lean_toolchain, status, answer_size, answer_hash })
    }
}

impl Submission {
    pub async fn create(
        pid: i32, submitter: &str, time: SystemTime,
        module_name: &str, const_name: &str, lean_toolchain: &str,
        answer_size: u64, answer_hash: [u8; 32],
        db: &mut Client,
    ) -> DBResult<u32> {
        const SQL: &str = "insert into lean4oj.submissions (pid, submitter, time, module_name, const_name, lean_toolchain, answer_size, answer_hash) values ($1, $2, $3, $4, $5, $6, $7, $8) returning sid";

        let stmt = db.prepare_static(SQL.into()).await?;
        let row = db.query_one(&stmt, &[
            &pid, &submitter, &time,
            &module_name, &const_name, &lean_toolchain,
            &answer_size.cast_signed(), &answer_hash.as_slice(),
        ]).await?;
        row.try_get::<_, i32>(0).map(i32::cast_unsigned)
    }

    pub async fn report_status(sid: u32, status: SubmissionStatus, db: &mut Client) -> DBResult<()> {
        const SQL: &str = "update lean4oj.submissions set status = $1 where sid = $2";

        let stmt = db.prepare_static(SQL.into()).await?;
        let n = db.execute(&stmt, &[&(status as u8).cast_signed(), &sid.cast_signed()]).await?;
        if n == 1 {
            Ok(())
        } else {
            Err(DBError::new(tokio_postgres::error::Kind::UnexpectedMessage, None))
        }
    }
}
