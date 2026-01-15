use std::borrow::Cow;

use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub enum Action {
    #[allow(clippy::enum_variant_names)]
    NoAction,
    Replace(Cow<'static, str>),
    Append(Cow<'static, str>),
}
