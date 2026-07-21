use std::time::{Duration, Instant};

use autd3_rs::{CycleOutcome, Geometry, IntoLink, Link, LinkError, RX_FRAME_BYTES, TX_FRAME_BYTES};
use autd3_rs_link_nop::Nop;

type Emulator = <Nop as IntoLink>::Link;

pub struct PacedNop {
    period: Duration,
}

impl PacedNop {
    pub const fn new(period: Duration) -> Self {
        Self { period }
    }
}

impl IntoLink for PacedNop {
    type Link = Paced;

    async fn into_link(self, geometry: &Geometry) -> Result<Paced, LinkError> {
        Ok(Paced {
            inner: Nop.into_link(geometry).await?,
            period: self.period,
            next: None,
        })
    }
}

pub struct Paced {
    inner: Emulator,
    period: Duration,
    next: Option<Instant>,
}

impl Link for Paced {
    type Error = <Emulator as Link>::Error;
    type Checker = <Emulator as Link>::Checker;

    fn num_devices(&self) -> usize {
        self.inner.num_devices()
    }

    fn state_checker(&self) -> Self::Checker {
        self.inner.state_checker()
    }

    fn cycle(
        &mut self,
        tx: &[[u8; TX_FRAME_BYTES]],
        rx: &mut [[u8; RX_FRAME_BYTES]],
    ) -> Result<CycleOutcome, Self::Error> {
        if !self.period.is_zero() {
            let now = Instant::now();
            let deadline = self.next.unwrap_or(now);
            if let Some(remaining) = deadline.checked_duration_since(now) {
                std::thread::sleep(remaining);
            }
            self.next = Some((deadline + self.period).max(Instant::now()));
        }
        self.inner.cycle(tx, rx)
    }
}
