use serde::Deserialize;

#[derive(Clone, Copy, Deserialize)]
#[serde(tag = "queryRepliesType")]
pub enum QueryRepliesType {
    #[serde(rename_all = "camelCase")]
    HeadTail { head_take_count: u64, tail_take_count: u64 },
    #[serde(rename_all = "camelCase")]
    IdRange { before_id: u32, after_id: u32, id_range_take_count: u64 },
}
