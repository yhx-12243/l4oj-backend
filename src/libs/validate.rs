use std::{borrow::Cow, sync::LazyLock};

use pcre2::bytes::{Regex, RegexBuilder};

use super::util::unescape_quoted;

pub fn check_username(username: &str) -> bool {
    3 <= username.len() && username.len() <= 24 && username.bytes().all(|x| matches!(x,  b'#' | b'$' | b'-' | b'.' | b'0'..=b'9' | b'A'..=b'Z' | b'_' | b'a'..=b'z'))
}

pub fn is_lean_id_first(ch: char) -> bool {
    if ch.is_alphabetic() { return true; }
    if ch == '_' { return true; }
    if matches!(ch, 'α'..='ϻ') && ch != 'λ' { return true; }
    if matches!(ch, 'Α'..='Ο' | 'Ρ' | 'Τ'..='Ω') { return true; }
    if matches!(ch, 'ἀ'..='῾' | '℀'..='⅏' | '𝒜'..='𝖟') { return true; }
    matches!(ch, 'À'..='ſ') && ch != '×' && ch != '÷'
}

pub fn is_lean_id_rest(ch: char) -> bool {
    if is_lean_id_first(ch) { return true; }
    if ch.is_ascii_digit() { return true; }
    if matches!(ch, '\'' | '!' | '?') { return true; }
    matches!(ch, '₀'..='ₜ' | 'ᵢ'..='ᵪ' | 'ⱼ')
}

pub fn check_uid(uid: &str) -> bool {
    let mut iter = uid.chars();
    let Some(first) = iter.next() else { return false; };
    if !is_lean_id_first(first) { return false; }
    matches!(
        iter.try_fold(0usize, |len, ch| if is_lean_id_rest(ch) { Some(len + 1) } else { None }),
        Some(2..24),
    )
}

struct EmailRegexs {
    REMOVE_COMMENT: Regex,
    LOCAL_PART: Regex,
    DOMAIN: Regex,
}

mod regex {
    use super::{Cow, Regex, RegexBuilder};

    pub fn create(pattern: &str) -> Regex {
        #[allow(clippy::unwrap_used)]
        RegexBuilder::new()
            .jit_if_available(true)
            .build(pattern)
            .unwrap()
    }

    pub fn remove<'a>(haystack: &'a str, regex: &Regex) -> Cow<'a, str> {
        let mut matches = regex.find_iter(haystack.as_bytes()).filter_map(Result::ok).peekable();

        let Some(first) = matches.next() else { return Cow::Borrowed(haystack) };
        let pre = unsafe { haystack.get_unchecked(..first.start()) };
        let second = matches.peek();
        if second.is_none() {
            if first.end() == haystack.len() {
                return Cow::Borrowed(pre);
            }
            if first.start() == 0 {
                return Cow::Borrowed(unsafe { haystack.get_unchecked(first.end()..) });
            }
        }

        let mut new = String::with_capacity(haystack.len() - (first.end() - first.start()));
        new.push_str(pre);
        let mut last = first.end();

        for mat in matches {
            new.push_str(unsafe { haystack.get_unchecked(last..mat.start()) });
            last = mat.end();
        }

        new.push_str(unsafe { haystack.get_unchecked(last..) });
        Cow::Owned(new)
    }
}

fn remove_comment(mut haystack: &str) -> &str {
    if let Some(s) = haystack.strip_prefix('(') && let Some(i) = s.find(')') {
        haystack = unsafe { s.get_unchecked(i + 1..) };
    }
    if let Some(s) = haystack.strip_suffix(')') && let Some(i) = s.rfind('(') {
        haystack = unsafe { s.get_unchecked(..i) };
    }
    haystack
}

pub fn check_email<'a>(email: &'a str) -> Option<(Cow<'a, str>, &'a str)> {
    static EMAIL_REGEXS: LazyLock<EmailRegexs> = LazyLock::new(|| EmailRegexs {
        REMOVE_COMMENT: regex::create(r"(?:^\([^)]*\))|(?:\([^)]*\))$"),
        LOCAL_PART: regex::create(r#"^(?:[^\s"(),.:;<>@[\\\]]+(?:\.[^\s"(),.:;<>@[\\\]]+)*)$"#),
        DOMAIN: regex::create(r"^(?:(?:\[(?:\d{1,3}\.){3}\d{1,3}])|(?:(?:[\dA-Za-z-]+\.)+[A-Za-z]{2,}))$"),
    });

    if email.is_empty() || email.contains(['«', '»']) || email.bytes().any(|x| x <= 32) {
        return None;
    }

    let (local_part, domain) = email.rsplit_once('@')?;

    let EmailRegexs {
        REMOVE_COMMENT: _,
        LOCAL_PART,
        DOMAIN,
    } = &*EMAIL_REGEXS;

    let domain: &'a str = remove_comment(domain);
    if domain.len() > 254 || domain.split('.').any(|part| part.len() > 63) { return None }

    let Ok(true) = DOMAIN.is_match(domain.as_bytes()) else { return None };

    let local_part: &'a str = remove_comment(local_part);
    if local_part.len() > 64 { return None }

    let local_part_1: Cow<'a, str> = if let [b'"', .., b'"'] = *local_part.as_bytes() {
        unescape_quoted(unsafe { local_part.get_unchecked(1..) }).ok()?
    } else {
        match LOCAL_PART.is_match(local_part.as_bytes()) {
            Ok(true) => Cow::Borrowed(local_part),
            _ => return None,
        }
    };

    #[allow(clippy::unwrap_used)]
    Some((local_part_1, domain))
}
