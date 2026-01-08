use compact_str::CompactString;
use hashbrown::HashMap;
use serde::Serialize;

#[derive(Default, Serialize)]
#[repr(transparent)]
pub struct Reaction(pub HashMap<CompactString, u32>);

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReactionAOE {
    pub count: Reaction,
    pub current_user_reactions: Vec<CompactString>,
}
