use std::sync::OnceLock;

use hashbrown::HashMap;

const DATA: [(&[u8], &str); 1] = [
    (b".26.0", "d8204c9fd894f91bbb2cdfec5912ec8196fd8562"),
];

static ACCEPTABLE_VERSIONS: OnceLock<HashMap<&[u8], &str>> = OnceLock::new();

pub fn init() {
    ACCEPTABLE_VERSIONS.get_or_init(|| HashMap::from(DATA));
}
