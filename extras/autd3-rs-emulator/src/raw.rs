#[derive(Clone, Debug, PartialEq)]
pub enum RawColumn {
    U8(Vec<u8>),
    U16(Vec<u16>),
    F32(Vec<f32>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct RawFrame {
    pub rows: usize,
    pub columns: Vec<(String, RawColumn)>,
}
