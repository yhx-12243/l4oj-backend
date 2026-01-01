use core::{ascii::Char, cell::UnsafeCell};
use std::borrow::Cow;

/// leading double quote is NOT included, the trailing one is REQUIRED.
///
/// e.g., `unescape_quoted("abc\"") => "abc"`. see [`tests::unescape`] for more details.
pub fn unescape_quoted(str: &str) -> Result<Cow<'_, str>, serde_json::Error> {
    use serde_json::de;

    let mut copied = String::new();

    let reader = UnsafeCell::new(de::StrRead::new(str));

    let res = unsafe { de::Read::parse_str(reader.as_mut_unchecked(), copied.as_mut_vec())? };

    let offset = de::Read::byte_offset(unsafe { reader.as_ref_unchecked() });
    if offset == str.len() {
        Ok(match res {
            de::Reference::Borrowed(b) => Cow::Borrowed(b),
            de::Reference::Copied(_) => Cow::Owned(copied),
        })
    } else {
        let position = de::Read::peek_position(unsafe { reader.as_ref_unchecked() });
        Err(serde_json::Error::syntax(
            serde_json::error::ErrorCode::TrailingCharacters,
            position.line,
            position.column,
        ))
    }
}

pub fn gen_random_ascii<const N: usize>() -> [Char; N] {
    use rand::RngCore;
    // const SAMPLER: UniformInt<u32> = UniformInt::new_inclusive(33, 126).unwrap();
    #[inline]
    fn g(rng: &mut impl RngCore) -> Char {
        unsafe { Char::from_u8_unchecked((((u64::from(rng.next_u32()) * 94) >> 32) + 33) as _) }
    }
    let mut rng = rand::rng();
    core::array::from_fn(|_| g(&mut rng))
}
