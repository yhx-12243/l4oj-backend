use compact_str::CompactString;

use crate::libs::validate::{is_lean_id_first, is_lean_id_rest};

pub fn normalize(mut raw: &str) -> Option<(bool, CompactString)> {
	let mut ret = CompactString::default();
	let mut dot = false;
	if let Some(s) = raw.strip_prefix('.') {
		dot = true;
		raw = s;
	}
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
	ok.then_some((dot, ret))
}
