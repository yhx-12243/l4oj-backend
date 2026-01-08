use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use tokio_postgres::{Client, Row};

use crate::libs::db::DBResult;

#[derive(Deserialize, Serialize)]
pub struct Information {
    pub organization: CompactString,
    pub location: CompactString,
    pub url: CompactString,
    pub telegram: CompactString,
    pub qq: CompactString,
    pub github: CompactString,
}

impl TryFrom<Row> for Information {
    type Error = tokio_postgres::Error;

    fn try_from(row: Row) -> Result<Self, Self::Error> {
        let organization = row.try_get::<_, &str>("organization")?.into();
        let location = row.try_get::<_, &str>("location")?.into();
        let url = row.try_get::<_, &str>("url")?.into();
        let telegram = row.try_get::<_, &str>("telegram")?.into();
        let qq = row.try_get::<_, &str>("qq")?.into();
        let github = row.try_get::<_, &str>("github")?.into();
        Ok(Self { organization, location, url, telegram, qq, github })
    }
}

impl Information {
    pub async fn of(uid: &str, conn: &mut Client) -> DBResult<Self> {
        const SQL: &str = "select organization, location, url, telegram, qq, github from lean4oj.user_information where uid = $1";

        let stmt = conn.prepare_static(SQL.into()).await?;
        conn.query_one(&stmt, &[&uid]).await?.try_into()
    }
}

