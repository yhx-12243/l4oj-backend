use core::mem;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
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
