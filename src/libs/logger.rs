use core::sync::atomic::{AtomicUsize, Ordering};

use env_logger::{
    TimestampPrecision,
    fmt::{
        ConfigurableFormat, Formatter,
        style::{AnsiColor, Style},
    },
};
use log::Record;
use parking_lot::Once;

#[inline]
fn pad_num(width: usize) -> usize {
    static MAX_MODULE_WIDTH: AtomicUsize = AtomicUsize::new(0);
    MAX_MODULE_WIDTH.fetch_max(width, Ordering::Relaxed).saturating_sub(width)
}

#[inline]
fn pad(s: &str, additional: usize) -> String {
    const TARGET_STYLE: Style = AnsiColor::BrightBlue.on_default();

    format!("{TARGET_STYLE}{s: <0$}{TARGET_STYLE:#}", s.len() + additional)
}

fn format(buf: &mut Formatter, record: &Record<'_>) -> std::io::Result<()> {
    const SPACES: &str = "                                                                                                                                ";

    const FMT: ConfigurableFormat = ConfigurableFormat {
        timestamp: Some(TimestampPrecision::Millis),
        module_path: true,
        target: true,
        level: true,
        indent: Some(4),
        suffix: "\n",
        source_file: false,
        source_line_number: false,
    };

    let module_path = record.module_path();
    let target = record.target();
    let target_w = if module_path == Some(target) {
        match pad_num(target.len()) {
            0 => "",
            1 => "\x1b[0m",
            n @ 2..=129 => &SPACES[..n - 1],
            n @ 130.. => &unsafe { String::from_utf8_unchecked(vec![b' '; n - 1]) },
        }
    } else {
        let len = module_path.map_or_default(str::len) + target.len() + 1;
        &pad(target, pad_num(len))
    };

    let record_w = Record::builder()
        .level(record.level())
        .target(target_w)
        .args(*record.args())
        .module_path(module_path)
        .file(record.file())
        .line(record.line())
        .build();

    FMT.format(buf, &record_w)
}

fn init_timed() {
    env_logger::builder().format(format).init();
}

pub fn init() {
    static LOGGER_INIT: Once = Once::new();
    LOGGER_INIT.call_once(init_timed);
}
