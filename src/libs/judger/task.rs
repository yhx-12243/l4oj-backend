use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

#[derive(Deserialize)]
#[repr(transparent)]
pub struct LeanAxiom {
    name: CompactString,
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(debug_assertions, derive(Debug))]
pub struct Task {
    pub sid: u32,
    pub version: CompactString,
    pub axioms: SmallVec<[CompactString; 4]>,
}
