use core::slice;

use futures_util::future::join_all;
use tokio::io::{AsyncBufRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub async fn c2s_multiplex<R, W>(mut client: R, mut handler: [Option<W>; 256])
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut buf = Vec::<u8>::new();
    let Err(e) = try {
        loop {
            let l = client.read_u32_le().await?;
            let len = (l & 0x00ff_ffff) as usize;
            tracing::debug!("receive block of channel \x1b[33m{}\x1b[0m, size \x1b[36m{}\x1b[0m", l >> 24, len);
            buf.reserve(len);
            let chunk = unsafe { slice::from_raw_parts_mut(buf.as_mut_ptr(), len) };
            client.read_exact(chunk).await?;
            if let Some(w) = unsafe { handler.get_unchecked_mut((l >> 24) as usize) } {
                w.write_all(chunk).await?;
            }
        }
    };
    tracing::warn!(target: "lean4rsync-multiplex", "multiplex error: {e}");
    let s = handler.iter_mut().filter_map(Option::as_mut).map(AsyncWriteExt::shutdown);
    join_all(s).await;
}
