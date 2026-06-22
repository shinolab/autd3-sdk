#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatternDataType {
    Foci { num_foci: u8, sound_speed: u16 },
    Raw,
}
