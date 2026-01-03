use std::sync::OnceLock;

use hashbrown::HashMap;

const DATA: [(&[u8], &[u8; 40]); 1] = [
    (b".26.0", b"d8204c9fd894f91bbb2cdfec5912ec8196fd8562"),
];

static ACCEPTABLE_VERSIONS: OnceLock<HashMap<&[u8], &[u8; 40]>> = OnceLock::new();

pub fn init() {
    ACCEPTABLE_VERSIONS.get_or_init(|| HashMap::from(DATA));
}

pub fn lean_version_80(header: &[u8; 80]) -> Option<&'static str> {
    const MAGIC: &[u8; 8] = b"olean\x02\x014";
    if unsafe { *header.as_ptr().cast_array() != *MAGIC } { return None; }
    let middle: &[u8; 32] = unsafe { &*header.as_ptr().add(8).cast_array() };
    let tail: &[u8; 40] = unsafe { &*header.as_ptr().add(40).cast_array() };
    let len = middle.iter().rposition(|&x| x != 0).map_or_default(|x| x + 1);
    let ver_shortlived = unsafe { middle.get_unchecked(..len) };
    let map = {
        #[cfg(feature = "build-std")]
        unsafe { ACCEPTABLE_VERSIONS.get_unchecked() }
        #[cfg(not(feature = "build-std"))]
        unsafe { ACCEPTABLE_VERSIONS.get().unwrap_unchecked() }
    };
    let (&ver_longlived, &hash) = map.get_key_value(ver_shortlived)?;
    (tail == hash).then_some(unsafe { core::str::from_utf8_unchecked(ver_longlived) })
}

#[allow(clippy::missing_const_for_fn)] // false positive.
pub fn lean_version(payload: &[u8]) -> Option<&'static str> {
    payload.first_chunk().and_then(lean_version_80)
}
