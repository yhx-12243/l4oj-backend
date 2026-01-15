use core::{
    num::NonZero,
    slice::{self, SliceIndex},
};
use std::io;

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, BufReader};

pub trait ReadVarintRsync {
    async fn read_varint<const BASE: usize>(&mut self) -> io::Result<u64>;
}

pub trait ReadPossibleLine {
    async fn read_possible_line<const C1: u8, const C2: u8>(&mut self) -> io::Result<*const [u8]>;
}

impl<R> ReadVarintRsync for R
where
    R: AsyncRead + Unpin,
{
    #[inline]
    async fn read_varint<const BASE: usize>(&mut self) -> io::Result<u64> {
        let first = self.read_u8().await?;
        let clz = match NonZero::new(!first) {
            Some(w) => w.leading_zeros(),
            None => return self.read_u64_le().await,
        };
        let then = clz as usize + BASE;
        let mut buf = [0u8; 16];
        self.read_exact(unsafe { slice::from_raw_parts_mut(buf.as_mut_ptr(), then) }).await?;
        unsafe { *buf.get_unchecked_mut(then) = first & (0x7f >> clz) };
        Ok(u64::from_le_bytes(*buf.first_chunk().unwrap()))
    }
}

impl<R> ReadPossibleLine for BufReader<R>
where
    R: AsyncRead + Unpin,
{
    async fn read_possible_line<const C1: u8, const C2: u8>(&mut self) -> io::Result<*const [u8]> {
        let s = self.fill_buf().await?;
        if let Some(len) = memchr::memchr2(C1, C2, s) {
            let b = unsafe { (..len).get_unchecked(s) };
            self.consume(len + 1);
            return Ok(b);
        }
        self.backshift();
        let mut l = self.buffer().len();
        while l < self.capacity() {
            let (unfilled, ptr, inner) = self.__read_more_internal();
            let m = inner.read(unfilled).await?;
            if m == 0 {
                let b = unsafe { (..l).get_unchecked(self.buffer()) };
                self.consume(l);
                return Ok(b);
            }
            *ptr += m;
            if let Some(n) = memchr::memchr2(C1, C2, self.buffer().get(l..).unwrap()) {
                let b = unsafe { (..(l + n)).get_unchecked(self.buffer()) };
                self.consume(l + n + 1);
                return Ok(b);
            }
            l += m;
            debug_assert_eq!(l, self.buffer().len());
        }
        Err(io::const_error!(io::ErrorKind::InvalidData, "line is too long"))
    }
}
