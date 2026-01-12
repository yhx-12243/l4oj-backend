use core::mem;

use serde::Serialize;

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
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
}

impl TryFrom<u8> for Status {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if value < 11 {
            unsafe { Ok(mem::transmute(value)) }
        } else {
            Err(())
        }
    }
}
