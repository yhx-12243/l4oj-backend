#![feature(
    const_format_args,
    exit_status_error,
    never_type,
    result_option_map_or_default,
    setgroups,
)]

#[path = "judger/constants.rs"]
mod constants;
#[path = "libs/logger.rs"]
mod logger;
#[path = "judger/main.rs"]
mod main;
#[path = "judger/task.rs"]
mod task;

#[tokio::main]
async fn main() -> ! {
    use hyper_util::rt::TokioIo;
    use tokio::net::UnixStream;

    const SOCK: &str = "lean4oj.sock";

    logger::init();

    loop {
        let stream = match UnixStream::connect(SOCK).await {
            Ok(sock)  => sock,
            Err(e) => {
                tracing::error!("Failed to connect to {SOCK}: {e}, reconnecting ...");
                tokio::time::sleep(constants::RECONNECT_INTERVAL).await;
                continue;
            }
        };
        let io = TokioIo::new(stream);

        if let Err(e) = main::main_loop(io).await {
            tracing::error!("Judger main loop exited with error: {e}");
        }
    }
}
