use core::{error, fmt};
use std::io;

use tokio::{
    io::{BufReader, BufWriter},
    net::unix::{OwnedReadHalf, OwnedWriteHalf},
};

use crate::libs::error::{BoxedStdError, DynStdError};

const INNER: io::Error = io::const_error!(io::ErrorKind::Unsupported, "operation not supported on this platform");

#[derive(Debug)]
struct UnsupportedReadError {
    sni: String,
    uid: String,
}

impl fmt::Display for UnsupportedReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Please use `wget -r -nH -np -R 'index.html,index.html.tmp' '{}/lean/{}/'` (no backticks) to download files.", self.sni, self.uid)
    }
}

impl error::Error for UnsupportedReadError {
    fn source(&self) -> Option<&DynStdError> {
        Some(const { &INNER })
    }
}

pub fn main(
    _c2s: BufReader<OwnedReadHalf>,
    _s2c: BufWriter<OwnedWriteHalf>,
    sni: String,
    uid: String,
) -> Result<(), BoxedStdError> {
    Err(UnsupportedReadError { sni, uid }.into())
}
