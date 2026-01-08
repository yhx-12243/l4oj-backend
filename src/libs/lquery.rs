use compact_str::CompactString;

use crate::libs::validate::{check_username_u8, is_lean_id_first, is_lean_id_rest};

fn normalize_ul(raw: &[u8]) -> Option<CompactString> {
    let mut ret = CompactString::default();
    let mut iter = raw.iter();
    let first = *iter.next()?;
    let mut s = if first == b'*' {
        ret.push('%');
        0
    } else if first == b'_' {
        ret.push_str("\\_");
        1
    } else if check_username_u8(first) {
        ret.push(first.into());
        1
    } else {
        return None;
    };
    for &rest in &mut iter {
        if rest == b'*' && s != 0 {
            ret.push('%');
            s = 2;
            break;
        } else if rest == b'_' {
            ret.push_str("\\_");
        } else if check_username_u8(rest) {
            ret.push(rest.into());
        } else {
            return None;
        }
        s = 1;
    }
    #[allow(unused_parens)] // false positive.
    let ok = matches!(s, 1 | (2 if iter.next().is_none()));
    ok.then_some(ret)
}

fn normalize_dot(raw: &str) -> Option<CompactString> {
    let mut ret = CompactString::default();
    let mut iter = raw.chars();
    let first = iter.next()?;
    let mut s = if first == '*' {
        ret.push('%');
        0
    } else if first == '_' {
        ret.push_str("\\_");
        1
    } else if is_lean_id_first(first) {
        ret.push(first);
        1
    } else {
        return None;
    };
    for rest in &mut iter {
        if rest == '*' && s != 0 {
            ret.push('%');
            s = 2;
            break;
        } else if rest == '_' {
            ret.push_str("\\_");
        } else if is_lean_id_rest(rest) {
            ret.push(rest);
        } else {
            return None;
        }
        s = 1;
    }
    #[allow(unused_parens)] // false positive.
    let ok = matches!(s, 1 | (2 if iter.next().is_none()));
    ok.then_some(ret)
}

pub fn normalize(raw: &str) -> Option<(bool, CompactString)> {
    if let Some(s) = raw.strip_prefix('.') {
        normalize_dot(s).map(|r| (true, r))
    } else {
        normalize_ul(raw.as_bytes()).map(|r| (false, r))
    }
}
