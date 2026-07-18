#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RtSchedulePolicy {
    Normal,
    #[default]
    Fifo,
    RoundRobin,
}
