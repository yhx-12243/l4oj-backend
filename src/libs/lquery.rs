use core::index::Last;
use std::borrow::Cow;

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

#[inline]
pub const fn 𝑛𝑒𝑒𝑑_𝑒𝑠𝑐𝑎𝑝𝑒(ch: u8) -> bool { matches!(ch, b'%' | b'\\' | b'_') }

pub fn 𝑒𝑠𝑐𝑎𝑝𝑒(s: &str) -> String {
    let c = s.bytes().filter(|&x| 𝑛𝑒𝑒𝑑_𝑒𝑠𝑐𝑎𝑝𝑒(x)).count();
    let mut buf = Vec::with_capacity(s.len() + c + 2);
    buf.push(b'%');
    for b in s.bytes() {
        if 𝑛𝑒𝑒𝑑_𝑒𝑠𝑐𝑎𝑝𝑒(b) { buf.push(b'\\'); }
        buf.push(b);
    }
    buf.push(b'%');
    unsafe { String::from_utf8_unchecked(buf) }
}

fn 𝑒𝑠𝑐𝑎𝑝𝑒_𝚛𝚊𝚠(s: &str) -> Cow<'_, str> {
    let c = s.bytes().filter(|&x| 𝑛𝑒𝑒𝑑_𝑒𝑠𝑐𝑎𝑝𝑒(x)).count();
    if c == 0 { return Cow::Borrowed(s); }
    let mut buf = Vec::with_capacity(s.len() + c);
    for b in s.bytes() {
        if 𝑛𝑒𝑒𝑑_𝑒𝑠𝑐𝑎𝑝𝑒(b) { buf.push(b'\\'); }
        buf.push(b);
    }
    unsafe { Cow::Owned(String::from_utf8_unchecked(buf)) }
}

pub fn 𝑒𝑠𝑐𝑎𝑝𝑒_𝚕𝚊𝚣𝚢(s: &str) -> Option<Cow<'_, str>> {
    if s.len() < 3 && s.bytes().all(|x| x == b'*') { return None; }
    let mut t = 𝑒𝑠𝑐𝑎𝑝𝑒_𝚛𝚊𝚠(s);
    if t.starts_with('*') {
        unsafe {
            *t.to_mut().as_mut_vec().get_unchecked_mut(0) = b'%';
        }
    }
    if t.ends_with('*') {
        unsafe {
            *t.to_mut().as_mut_vec().get_unchecked_mut(Last) = b'%';
        }
    }
    Some(t)
}

pub mod jsonb {
    use std::borrow::Cow;

    const fn need_escape_regex(ch: u8) -> bool {
        matches!(ch, b'$' | b'('..=b'+' | b'.' | b'?' | b'['..=b'^' | b'{'..=b'}')
    }

    fn make_jsonb_path_match_query_inner(s: &str) -> Cow<'_, str> {
        let c = s.bytes().filter(|&x| need_escape_regex(x)).count();
        if c == 0 { return Cow::Borrowed(s); }
        let mut buf = Vec::with_capacity(s.len() + c);
        for b in s.bytes() {
            if need_escape_regex(b) { buf.push(b'\\'); }
            buf.push(b);
        }
        unsafe { Cow::Owned(String::from_utf8_unchecked(buf)) }
    }

    pub fn make_jsonb_path_match_query(s: &str) -> String {
        let mut ret = "$.*.title like_regex ".to_owned();
        let inner = make_jsonb_path_match_query_inner(s);
        let _ = serde_json::to_writer(unsafe { ret.as_mut_vec() }, &*inner);
        ret.push_str(" flag \"i\"");
        ret
    }
}
