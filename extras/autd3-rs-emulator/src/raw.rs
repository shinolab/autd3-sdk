pub enum RawColumn {
    U8(Vec<u8>),
    U16(Vec<u16>),
    F32(Vec<f32>),
}

pub struct RawFrame {
    pub rows: usize,
    pub columns: Vec<(String, RawColumn)>,
}

#[cfg(feature = "polars")]
impl RawFrame {
    pub(crate) fn into_polars(self) -> polars::frame::DataFrame {
        use polars::prelude::Column;
        let columns = self
            .columns
            .into_iter()
            .map(|(name, col)| match col {
                RawColumn::U8(v) => Column::new(name.into(), v.as_slice()),
                RawColumn::U16(v) => Column::new(name.into(), v.as_slice()),
                RawColumn::F32(v) => Column::new(name.into(), v.as_slice()),
            })
            .collect::<Vec<_>>();
        polars::frame::DataFrame::new(self.rows, columns).unwrap()
    }
}
