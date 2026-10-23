mod aabb;
mod error;
mod output_ultrasound;
mod output_voltage;
mod range;
mod raw;
mod record;
mod recorder;
mod sound_field;

pub use error::{EmulatorError, GridAxis, InvalidGrid};
pub use range::{AxisOrder, Grid, Range};
pub use raw::{RawColumn, RawFrame};
pub use record::Record;
pub use recorder::Recorder;
pub use sound_field::{Instant, InstantRecordOption, Rms, RmsRecordOption, SoundFieldOption};

use autd3_rs_core::geometry::Geometry;

pub struct Emulator {
    geometry: Geometry,
}

impl Emulator {
    #[must_use]
    pub fn new(geometry: Geometry) -> Self {
        Self { geometry }
    }

    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn transducer_table(&self) -> RawFrame {
        let n = self.geometry.num_transducers();
        let mut dev_idx = Vec::with_capacity(n);
        let mut tr_idx = Vec::with_capacity(n);
        let (mut x, mut y, mut z) = (Vec::new(), Vec::new(), Vec::new());
        let (mut nx, mut ny, mut nz) = (Vec::new(), Vec::new(), Vec::new());
        for (dev, device) in self.geometry.iter().enumerate() {
            let dir = device.axial_direction();
            for (tr, p) in device.positions().iter().enumerate() {
                dev_idx.push(dev as u16);
                tr_idx.push(tr as u8);
                x.push(p.x);
                y.push(p.y);
                z.push(p.z);
                nx.push(dir.x);
                ny.push(dir.y);
                nz.push(dir.z);
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

    pub fn record<F>(&self, f: F) -> Result<Record, EmulatorError>
    where
        F: FnOnce(&mut Recorder) -> Result<(), EmulatorError>,
    {
        let mut recorder = Recorder::new(&self.geometry, 0);
        f(&mut recorder)?;
        Ok(recorder.into_record())
    }
}
