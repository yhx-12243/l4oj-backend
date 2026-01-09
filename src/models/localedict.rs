use compact_str::CompactString;
use hashbrown::HashMap;
use serde::{Deserialize, Serialize, ser::SerializeSeq};

#[derive(Debug, Deserialize)]
#[repr(transparent)]
pub struct LocaleDict<T = CompactString>(pub HashMap<CompactString, T>);

impl<T> LocaleDict<T> {
    pub fn apply(&self, locale: Option<&str>) -> Option<&T> {
        if let Some(locale) = locale && let Some(res) = self.0.get(locale) {
            Some(res)
        } else if let Some(res) = self.0.get("zh_CN") {
            Some(res)
        } else {
            self.0.values().next()
        }
    }

    pub fn apply_with_key(&self, locale: Option<&str>) -> Option<(&CompactString, &T)> {
        if let Some(locale) = locale && let Some(res) = self.0.get_key_value(locale) {
            Some(res)
        } else if let Some(res) = self.0.get_key_value("zh_CN") {
            Some(res)
        } else {
            self.0.iter().next()
        }
    }

    pub fn apply_owned(mut self, locale: Option<&str>) -> Option<T> {
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
pub struct LocaleDictEntry<'a, T> {
    pub locale: &'a str,
    pub name: &'a T,
}

#[derive(Serialize)]
pub struct LocaleDictEntryFlatten<'a, T> {
    pub locale: &'a str,
    #[serde(flatten)]
    pub field: &'a T,
}

#[derive(Deserialize)]
pub struct LocaleDictEntryOwned {
    pub locale: CompactString,
    pub name: CompactString,
}

#[derive(Deserialize)]
pub struct LocaleDictEntryOwnedFlatten<T> {
    pub locale: CompactString,
    #[serde(flatten)]
    pub field: T,
}

impl FromIterator<LocaleDictEntryOwned> for LocaleDict {
    fn from_iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = LocaleDictEntryOwned>,
    {
        Self(iter.into_iter()
            .map(|LocaleDictEntryOwned { locale, name }| (locale, name))
            .collect())
    }
}

impl<T> FromIterator<LocaleDictEntryOwnedFlatten<T>> for LocaleDict<T> {
    fn from_iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = LocaleDictEntryOwnedFlatten<T>>,
    {
        Self(iter.into_iter()
            .map(|LocaleDictEntryOwnedFlatten { locale, field }| (locale, field))
            .collect())
    }
}

impl<T> Serialize for LocaleDict<T>
where
    T: Serialize,
{
    default fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
        for (locale, field) in &self.0 {
            seq.serialize_element(&LocaleDictEntryFlatten { locale, field })?;
        }
        seq.end()
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