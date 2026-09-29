use autd3_rs_core::{BusStats, CycleOutcome, DcClock, RX_FRAME_BYTES, TX_FRAME_BYTES};

pub(crate) trait Bus: Send + 'static {
    type Error: core::error::Error + Send + Sync + 'static;

    fn num_devices(&self) -> usize;

    fn stats(&self) -> BusStats {
        BusStats::default()
    }

    fn dc_clock(&self) -> Option<DcClock> {
        None
    }

    fn wait_next_cycle(&mut self) {}

    fn cycle(
        &mut self,
        tx: &[[u8; TX_FRAME_BYTES]],
        rx: &mut [[u8; RX_FRAME_BYTES]],
    ) -> Result<CycleOutcome, Self::Error>;

    fn close(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}
