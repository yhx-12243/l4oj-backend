#![allow(clippy::cast_ptr_alignment)]
#![cfg(target_pointer_width = "64")]

#[cfg(test)]
use core::fmt;
use std::sync::OnceLock;

use compact_str::CompactString;
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
    (*tail == *hash).then_some(unsafe { core::str::from_utf8_unchecked(ver_longlived) })
}

#[allow(clippy::missing_const_for_fn)] // false positive.
pub fn lean_version(payload: &[u8]) -> Option<&'static str> {
    payload.first_chunk().and_then(lean_version_80)
}

#[derive(Clone, Copy)]
pub struct OleanMeta<'a> {
    data: &'a [u8],
    version: &'static str,
    base: usize,
    sections: &'a [usize; 7],
}

#[cfg(test)]
impl fmt::Debug for OleanMeta<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> core::fmt::Result {
        let f_sections = |fmt: &mut fmt::Formatter<'_>| {
            fmt.debug_list()
                .entries(
                    self.sections.iter().map(|x|
                        fmt::from_fn(|fmt| fmt::LowerHex::fmt(x, fmt)))
                )
                .finish()
        };
        let f_data = |fmt: &mut fmt::Formatter<'_>| {
            fmt.debug_map()
                .entry(&"size", &self.data.len())
                .finish()
        };
        f.debug_struct_field3_finish(
            "OleanMeta",
            "data", &fmt::from_fn(f_data),
            "base", &fmt::from_fn(|fmt| fmt::LowerHex::fmt(&self.base, fmt)),
            "sections", &fmt::from_fn(f_sections),
        )
    }
}

mod detail {
    use core::{slice, str};

    use compact_str::CompactString;

    use super::super::validate::is_lean_id;

    pub(super) fn array(payload: &[u8], offset: usize) -> Option<&[usize]> {
        let header = unsafe { &*payload.get(offset..offset + 24)?.as_ptr().cast::<[usize; 3]>() };
        if header[0] != 0xf600_0001_0000_0000 || header[1] != header[2] { return None; }
        let n = header[1];
        let start = offset + 24;
        if start.checked_add(n.checked_mul(8)?)? > payload.len() { return None; }
        Some(unsafe { slice::from_raw_parts(payload.as_ptr().add(start).cast::<usize>(), n) })
    }

    #[inline]
    fn is_hcongr_reserved_name_suffix(s: &str) -> bool {
        if let Some(suffix) = s.strip_prefix("hcongr_") {
            suffix.chars().all(|c| c.is_ascii_digit())
        } else {
            false
        }
    }

    #[inline]
    fn is_internal(s: &str) -> bool {
        s.starts_with('_') ||
        s.starts_with("eq_") ||
        s.starts_with("match_") ||
        s.starts_with("omega_") ||
        s.starts_with("proof_") ||
        s == "congr_simp" ||
        is_hcongr_reserved_name_suffix(s)
    }

    pub(super) fn str(payload: &[u8], base: usize, addr: usize) -> Option<&str> {
        let p = addr - base;
        let header = unsafe { &*payload.get(p..p + 32)?.as_ptr().cast::<[usize; 4]>() };
        if header[0] != 0xf900_0001_0000_0000 { return None; }
        let len = header[3];
        str::from_utf8(payload.get(p + 32..(p + 32).checked_add(len)?)?).ok()
    }

    pub(super) fn name(payload: &[u8], base: usize, addr: usize) -> Option<CompactString> {
        let p = addr - base;
        let header = unsafe { &*payload.get(p..p + 32)?.as_ptr().cast::<[usize; 4]>() };
        if header[0] != 0x102_0020_0000_0000 { return None; }
        let recur = header[1];
        let mut ret = if recur == 1 {
            CompactString::default()
        } else {
            let mut prefix = name(payload, base, recur)?;
            prefix.push('.');
            prefix
        };
        let strg = header[2];
        let last = str(payload, base, strg)?;
        if !is_lean_id(last) || is_internal(last) { return None; }

        ret.push_str(last);
        Some(ret)
    }
}

pub fn parse_meta(payload: &[u8]) -> Option<OleanMeta<'_>> {
    let version = lean_version(payload)?;

    let addr = usize::from_le_bytes(*payload.get(88..96)?.as_array()?);
    let base = usize::from_le_bytes(unsafe { *payload.as_ptr().add(80).cast_array() });
    let offset = addr - base;
    if offset + 56 != payload.len() { return None; }
    let sections = unsafe { &*payload.as_ptr().add(offset).cast::<[usize; 7]>() };
    if sections[0] != 0x5_0038_0000_0000 { return None; }

    Some(OleanMeta { data: payload, version, base, sections })
}

pub fn parse_consts(OleanMeta { data, base, sections, .. }: OleanMeta<'_>) -> Option<Vec<CompactString>> {
    let raw = detail::array(data, sections[2] - base)?;

    let mut consts = Vec::with_capacity(raw.len());
    for &raw_const in raw {
        if let Some(name) = detail::name(data, base, raw_const) {
            consts.push(name);
        }
    }
    consts.sort_unstable();
    consts.dedup();
    Some(consts)
}

pub fn parse_imports(OleanMeta { data, base, sections, .. }: OleanMeta<'_>) -> Option<Vec<CompactString>> {
    let raw = detail::array(data, sections[1] - base)?;

    let mut imports = Vec::with_capacity(raw.len());
    for &raw_import in raw {
        let p = raw_import - base;
        let ind = usize::from_le_bytes(*data.get(p + 8..p + 16)?.as_array()?);
        if let Some(name) = detail::name(data, base, ind) {
            imports.push(name);
        }
    }
    imports.sort_unstable();
    imports.dedup();
    Some(imports)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{init, parse_consts, parse_imports, parse_meta};

    const OLEANS: [&str; 16] = [
        "/Volumes/caseSensitive/test/server_frontend/public/lean/topology/cardinal_A₂.olean",
        "/Volumes/caseSensitive/test/server_frontend/public/lean/algebra/ring/Dec26.olean",
        "/Volumes/caseSensitive/test/server_frontend/public/lean/analysis/real/hasRootOddDegree.olean",
        "/Volumes/caseSensitive/test/server_frontend/public/lean/topology/Filter1.olean",
        "/Volumes/caseSensitive/test/server_frontend/public/lean/analysis/measure/Cantor.olean",
        "/Volumes/caseSensitive/test/server_frontend/public/lean/algebra/field/separable_perfect.olean",
        "/Volumes/caseSensitive/test/server_frontend/public/lean/analysis/measure/continuum.olean",
        "/Volumes/caseSensitive/test/server_frontend/public/lean/algebra/field/algclosure_realclosed.olean",
        "/Volumes/caseSensitive/test/server_frontend/public/lean/algebra/field/cos2π_9.olean",
        "/Volumes/caseSensitive/test/server_frontend/public/lean/IMO2025/IMO1989P1.olean",
        "/Volumes/caseSensitive/test/server_frontend/public/lean/algebra/field/2dimext.olean",
        "/Volumes/caseSensitive/test/server_frontend/public/lean/analysis/real/AC.olean",
        "/Volumes/caseSensitive/test/server_frontend/public/lean/analysis/real/Wirtinger.olean",
        "/tmp/lean/lean/Mathlib/FieldTheory/Isaacs.olean",
        "/tmp/lean/lean/Mathlib/FieldTheory/Isaacs.olean.server",
        "/tmp/lean/lean/Mathlib/FieldTheory/Isaacs.olean.private",
    ];

    #[test]
    fn test_parse() {
        init();

        for path in OLEANS {
            println!("{path}");
            let olean = fs::read(path).unwrap();
            let Some(meta) = parse_meta(&olean) else { continue };
            dbg!(parse_consts(meta));
            dbg!(parse_imports(meta));
        }
    }
}
