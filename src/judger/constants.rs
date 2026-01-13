use http::HeaderValue;

pub const USERNAME: &str = env!("LEAN4OJ_JUDGER_USERNAME");
pub const PASSWORD: &str = env!("LEAN4OJ_JUDGER_PASSWORD");

pub const DUMMY_HOST: HeaderValue = HeaderValue::from_static("judger");
pub const APPLICATION_JSON_UTF_8: HeaderValue = HeaderValue::from_static("application/json; charset=utf-8");
