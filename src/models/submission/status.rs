use core::mem;

use serde::{Deserialize, Serialize};
use tokio_postgres::types::{FromSql, Type, accepts};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum Status {
    Pending,

    Depositing,
    Deposited,
    JudgerReceived,
    TypeChecking,
    AxiomChecking,
    Replaying,

    InvalidImport,
    WrongAnswer,
    Accepted,
    JudgementFailed,
    Canceled,
}

impl TryFrom<u8> for Status {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if value < 12 {
            unsafe { Ok(mem::transmute::<u8, Self>(value)) }
        } else {
            Err(())
        }
    }
}

impl FromSql<'_> for Status {
    fn from_sql(_: &Type, raw: &[u8]) -> Result<Self, Box<dyn core::error::Error + Send + Sync + 'static>> {
        if let Some(&x) = raw.first() && let Ok(status) = x.try_into() {
            Ok(status)
        } else {
            Err("cannot decode Status".into())
        }
    }

    accepts!(CHAR);
}
