#![feature(
    const_format_args,
    exit_status_error,
    result_option_map_or_default,
    never_type,
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
async fn main() -> std::io::Result<!> {
    use hyper_util::rt::TokioIo;
    use tokio::net::UnixStream;

    const SOCK: &str = "lean4oj.sock";

    logger::init();

    let stream = UnixStream::connect(SOCK).await?;
    let io = TokioIo::new(stream);

    main::main_loop(io).await
}
