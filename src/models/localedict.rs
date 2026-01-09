use core::ops::Deref;

use compact_str::CompactString;
use hashbrown::HashMap;
use serde::{Deserialize, Serialize, ser::SerializeSeq};

#[derive(Debug, Deserialize)]
#[repr(transparent)]
pub struct LocaleDict(pub HashMap<CompactString, CompactString>);

impl LocaleDict {
    pub fn apply(&self, locale: Option<&str>) -> &str {
        if let Some(locale) = locale && let Some(res) = self.0.get(locale) {
            res
        } else if let Some(res) = self.0.get("zh_CN") {
            res
        } else {
            self.0.values().next().map_or_default(Deref::deref)
        }
    }

    pub fn apply_owned(mut self, locale: Option<&str>) -> Option<CompactString> {
        if let Some(locale) = locale && let Some(res) = self.0.remove(locale) {
            Some(res)
        } else if let Some(res) = self.0.remove("zh_CN") {
            Some(res)
        } else {
            self.0.into_values().next()
        }
    }
}

#[derive(Serialize)]
struct LocaleDictEntry<'a> {
    locale: &'a str,
    name: &'a str,
}

#[derive(Deserialize)]
pub struct LocaleDictEntryOwned {
    locale: CompactString,
    name: CompactString,
}

impl FromIterator<LocaleDictEntryOwned> for LocaleDict {
    fn from_iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = LocaleDictEntryOwned>,
    {
        Self(
            iter.into_iter()
                .map(|LocaleDictEntryOwned { locale, name }| (locale, name))
                .collect(),
        )
    }
}

impl Serialize for LocaleDict {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
        for (locale, name) in &self.0 {
            seq.serialize_element(&LocaleDictEntry { locale, name })?;
        }
        seq.end()
    }
}
