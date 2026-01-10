use tokio_postgres::Client;

use crate::libs::db::DBResult;

pub async fn identifier(id: &str, conn: &mut Client) -> DBResult<bool> {
    const SQL: &str = "select from lean4oj.users where uid = $1";

    let stmt = conn.prepare_static(SQL.into()).await?;
    Ok(conn.query_opt(&stmt, &[&id]).await?.is_none())
}

pub async fn email(email: &str, conn: &mut Client) -> DBResult<bool> {
    const SQL: &str = "select from lean4oj.users where email = $1";

    let stmt = conn.prepare_static(SQL.into()).await?;
    Ok(conn.query_opt(&stmt, &[&email]).await?.is_none())
}
