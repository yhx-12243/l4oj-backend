#[derive(Clone, Copy)]
pub enum Aoe {
    Global,
    After(u32),
    Before(u32),
}
