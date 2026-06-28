mod aabb;
mod client_api;
mod error;
mod output_ultrasound;
mod output_voltage;
mod range;
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
pub use record::Record;
pub use recorder::Recorder;
pub use sound_field::{Instant, InstantRecordOption, Rms, RmsRecordOption, SoundFieldOption};

use autd3_rs_core::geometry::Geometry;

#[cfg(feature = "polars")]
use polars::df;
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

    #[cfg(feature = "polars")]
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn transducer_table(&self) -> DataFrame {
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
        df!(
            "dev_idx" => &dev_idx,
            "tr_idx" => &tr_idx,
            "x[mm]" => &x,
            "y[mm]" => &y,
            "z[mm]" => &z,
            "nx" => &nx,
            "ny" => &ny,
            "nz" => &nz,
        )
        .unwrap()
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
        let mut recorder = Recorder::open(&self.geometry, start_ns);
        pollster::block_on(f(&mut recorder))?;
        Ok(recorder.into_record())
    }
}
