//! Hardware-free emulator for
//! [AUTD3](https://hapislab.org/en/airborne-ultrasound-tactile-display): records what the devices
//! would emit, then computes the resulting ultrasound sound field offline.
//!
//! Use it to inspect a pattern or a modulation without a device on the desk.
//!
//! See the [documentation site](https://shinolab.github.io/autd3-sdk/en/).

mod aabb;
mod client_api;
mod error;
mod output_ultrasound;
mod output_voltage;
mod range;
mod raw;
mod record;
mod recorder;
mod sound_field;

pub use aabb::Aabb;
pub use client_api::ClientApi;
pub use error::EmulatorError;
pub use range::{
    Range, RangeX, RangeXY, RangeXYZ, RangeXZ, RangeXZY, RangeY, RangeYX, RangeYXZ, RangeYZ,
    RangeYZX, RangeZ, RangeZX, RangeZXY, RangeZY, RangeZYX,
};
pub use raw::{RawColumn, RawFrame};
pub use record::Record;
pub use recorder::Recorder;
pub use sound_field::{Instant, InstantRecordOption, Rms, RmsRecordOption, SoundFieldOption};

use autd3_rs_core::geometry::Geometry;

#[cfg(feature = "polars")]
use polars::frame::DataFrame;

pub struct Emulator {
    geometry: Geometry,
}

impl Emulator {
    #[must_use]
    pub fn new(geometry: Geometry) -> Self {
        Self { geometry }
    }

    #[must_use]
    pub fn geometry(&self) -> &Geometry {
        &self.geometry
    }

    pub fn geometry_mut(&mut self) -> &mut Geometry {
        &mut self.geometry
    }

    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn transducer_table_raw(&self) -> RawFrame {
        let n = self.geometry.num_transducers();
        let mut dev_idx = Vec::with_capacity(n);
        let mut tr_idx = Vec::with_capacity(n);
        let (mut x, mut y, mut z) = (Vec::new(), Vec::new(), Vec::new());
        let (mut nx, mut ny, mut nz) = (Vec::new(), Vec::new(), Vec::new());
        for (dev, device) in self.geometry.iter().enumerate() {
            for (tr, (p, d)) in device
                .positions()
                .iter()
                .zip(device.directions().iter())
                .enumerate()
            {
                dev_idx.push(dev as u16);
                tr_idx.push(tr as u8);
                x.push(p.x);
                y.push(p.y);
                z.push(p.z);
                nx.push(d.x);
                ny.push(d.y);
                nz.push(d.z);
            }
        }
        RawFrame {
            rows: n,
            columns: vec![
                ("dev_idx".to_string(), RawColumn::U16(dev_idx)),
                ("tr_idx".to_string(), RawColumn::U8(tr_idx)),
                ("x[mm]".to_string(), RawColumn::F32(x)),
                ("y[mm]".to_string(), RawColumn::F32(y)),
                ("z[mm]".to_string(), RawColumn::F32(z)),
                ("nx".to_string(), RawColumn::F32(nx)),
                ("ny".to_string(), RawColumn::F32(ny)),
                ("nz".to_string(), RawColumn::F32(nz)),
            ],
        }
    }

    #[cfg(feature = "polars")]
    #[must_use]
    pub fn transducer_table(&self) -> DataFrame {
        self.transducer_table_raw().into_polars()
    }

    pub fn record<F>(&self, f: F) -> Result<Record, EmulatorError>
    where
        F: AsyncFnOnce(&mut Recorder) -> Result<(), EmulatorError>,
    {
        self.record_from(0, f)
    }

    pub fn record_from<F>(&self, start_ns: u64, f: F) -> Result<Record, EmulatorError>
    where
        F: AsyncFnOnce(&mut Recorder) -> Result<(), EmulatorError>,
    {
        let mut recorder = Recorder::new(&self.geometry, start_ns);
        pollster::block_on(f(&mut recorder))?;
        Ok(recorder.into_record())
    }
}
