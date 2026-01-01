use core::{mem, slice};
use std::io;

use futures_util::TryFutureExt;
use tokio::io::{AsyncRead, AsyncReadExt};

#[inline]
fn emit_inner(value: u32, origin: u32, dest: &mut [u8]) -> &mut [u8] {
    match value.wrapping_sub(origin) {
        delta @ ..254 => {
            unsafe { core::hint::assert_unchecked(!dest.is_empty()); }
            dest[0] = delta as u8;
            &mut dest[..1]
        }
        delta @ ..0x8000 => {
            unsafe { core::hint::assert_unchecked(dest.len() >= 3); }
            dest[0] = 254;
            dest[1] = (delta >> 8) as u8;
            dest[2] = delta as u8;
            &mut dest[..3]
        },
        _ => {
            unsafe { core::hint::assert_unchecked(dest.len() >= 5); }
            dest[0] = 254;
            dest[1] = (value >> 24 | 0x80) as u8;
            dest[2] = value as u8;
            dest[3] = (value >> 8) as u8;
            dest[4] = (value >> 16) as u8;
            &mut dest[..5]
        }
    }
}

async fn recv_inner<R>(mut rx: R, origin: u32) -> io::Result<u32>
where
    R: AsyncRead + Unpin,
{
    let first = rx.read_u8().await?;
    match first {
        0 => Err(io::Error::from_raw_os_error(1349)),
        1..254 => Ok(origin.wrapping_add(u32::from(first))),
        254 => {
            let x = rx.read_u16().await?;
            if let Some(y) = x.checked_sub(0x8000) {
                let mut z = u32::from(y) << 16 | u32::from(y);
                rx.read_exact(unsafe { slice::from_raw_parts_mut((&raw mut z).cast::<u8>().add(1), 2) }).await?;
                Ok(z)
            } else {
                Ok(origin.wrapping_add(u32::from(x)))
            }
        }
        255 => Err(io::const_error!(io::ErrorKind::InvalidData, "Unexpected negative jumping."))
    }
}

#[derive(Default)]
pub struct Jumping {
    state: u32 = u32::MAX,
}

impl Jumping {
    #[inline]
    pub fn emit<'a>(&mut self, idx: u32, dest: &'a mut [u8]) -> &'a mut [u8] {
        emit_inner(idx, mem::replace(&mut self.state, idx), dest)
    }

    #[inline]
    pub fn recv<R>(&mut self, rx: R) -> impl Future<Output = io::Result<u32>>
    where
        R: AsyncRead + Unpin,
    {
        recv_inner(rx, self.state).map_ok(|v| {
            self.state = v;
            self.state
        })
    }
}
