use futures_util::FutureExt;
use tokio::{
    io::AsyncWriteExt,
    net::{UnixListener, UnixStream},
};

async fn handle(mut socket: UnixStream) -> std::io::Result<()> {
    socket.write_all(b"Hello from rsync service!\n").await?;
    Ok(())
}

pub async fn main() -> std::io::Result<!> {
    const SOCK: &str = "lean4rsync.sock";

    if let Err(err) = std::fs::remove_file(SOCK) && err.kind() != std::io::ErrorKind::NotFound {
        return Err(err);
    }

    let listener = UnixListener::bind(SOCK)?;

    loop {
        let socket = match listener.accept().await {
            Ok((socket, _)) => socket,
            Err(e) => {
                tracing::warn!("server accept error: {e:?}");
                continue;
            }
        };

        tokio::spawn(handle(socket).map(|r| if let Err(e) = r {
            tracing::warn!("failed to handle rsync client: {e:?}");
        }));
    }
}
