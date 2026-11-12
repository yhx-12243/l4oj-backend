use std::borrow::Cow;

pub enum Action {
    NoAction,
    Replace(Cow<'static, str>),
    Append(Cow<'static, str>),
}
