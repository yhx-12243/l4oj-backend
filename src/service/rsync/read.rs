use std::io;

use tokio::{
    io::{BufReader, BufWriter},
    net::unix::{OwnedReadHalf, OwnedWriteHalf},
};

use crate::libs::error::BoxedStdError;

pub fn main(
    _c2s: BufReader<OwnedReadHalf>,
    _s2c: BufWriter<OwnedWriteHalf>,
) -> Result<(), BoxedStdError> {
    Err(io::const_error!(io::ErrorKind::Unsupported, "operation not supported on this platform").into())
}
