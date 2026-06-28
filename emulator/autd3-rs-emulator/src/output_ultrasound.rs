#![allow(clippy::unreadable_literal)]

#[cfg(feature = "polars")]
use polars::frame::DataFrame;

use crate::record::{OUTPUT_VOLTAGE, Record, TS, TransducerRecord};

struct T4010A1BVDModel {
    state: (f32, f32, f32),
    last_v: f32,
}

impl T4010A1BVDModel {
    const CS: f32 = 200e-9;
    const L: f32 = 80e-6;
    const R: f32 = 0.7;
    const CP: f32 = 2700e-9;
    const RD: f32 = 150e-3;
    const H: f32 = TS;
    const NORMALIZE: f32 = 0.057430573;

    fn f0(y: (f32, f32, f32)) -> f32 {
        y.1
    }

    fn f1(v: f32, y: (f32, f32, f32)) -> f32 {
        -y.0 / (Self::L * Self::CS)
            - (Self::R + Self::RD) / Self::L * y.1
            - Self::RD / Self::L * y.2
            + v / Self::L
    }

    fn f2(&self, v: f32, y: (f32, f32, f32)) -> f32 {
        let dt = (v - self.last_v) / Self::H * 2.;
        y.0 / (Self::L * Self::CS)
            + (Self::R + Self::RD) / Self::L * y.1
            + (Self::RD / Self::L - 1. / (Self::RD * Self::CP)) * y.2
            + 1. / Self::RD * dt
            - v / Self::L
    }

    fn rk4(&mut self, input: f32) -> f32 {
        let state = self.state;
        let y = state.1 * Self::NORMALIZE;

        let k00 = Self::H * Self::f0(state);
        let k01 = Self::H * Self::f1(self.last_v, state);
        let k02 = Self::H * self.f2(self.last_v, state);
        let y1 = (state.0 + k00 / 2., state.1 + k01 / 2., state.2 + k02 / 2.);

        let v = f32::midpoint(self.last_v, input);
        let k10 = Self::H * Self::f0(y1);
        let k11 = Self::H * Self::f1(v, y1);
        let k12 = Self::H * self.f2(v, y1);
        let y2 = (state.0 + k10 / 2., state.1 + k11 / 2., state.2 + k12 / 2.);

        let k20 = Self::H * Self::f0(y2);
        let k21 = Self::H * Self::f1(v, y2);
        let k22 = Self::H * self.f2(v, y2);
        let y3 = (state.0 + k20, state.1 + k21, state.2 + k22);

        self.last_v = v;
        let k30 = Self::H * Self::f0(y3);
        let k31 = Self::H * Self::f1(input, y3);
        let k32 = Self::H * self.f2(input, y3);

        self.last_v = input;
        self.state = (
            state.0 + (k00 + 2. * k10 + 2. * k20 + k30) / 6.,
            state.1 + (k01 + 2. * k11 + 2. * k21 + k31) / 6.,
            state.2 + (k02 + 2. * k12 + 2. * k22 + k32) / 6.,
        );
        y
    }
}

impl TransducerRecord {
    pub(crate) fn output_ultrasound(&self) -> Vec<f32> {
        let mut it = self.output_ultrasound_iter();
        it.next_frames(self.pulse_width.len()).unwrap_or_default()
    }

    pub(crate) fn output_ultrasound_iter(&self) -> OutputUltrasound<'_> {
        OutputUltrasound {
            cursor: 0,
            record: self,
            model: T4010A1BVDModel {
                state: (0., 0., 0.),
                last_v: -OUTPUT_VOLTAGE,
            },
        }
    }
}

pub(crate) struct OutputUltrasound<'a> {
    cursor: usize,
    record: &'a TransducerRecord,
    model: T4010A1BVDModel,
}

impl OutputUltrasound<'_> {
    pub(crate) fn next_frames(&mut self, n: usize) -> Option<Vec<f32>> {
        let voltage = self.record.output_voltage_within(self.cursor, n)?;
        self.cursor += n;
        Some(voltage.into_iter().map(|v| self.model.rk4(v)).collect())
    }
}

impl Record {
    #[must_use]
    pub fn output_ultrasound_of(&self, transducer: usize) -> Vec<f32> {
        self.records[transducer].output_ultrasound()
    }

    #[cfg(feature = "polars")]
    #[must_use]
    pub fn output_ultrasound(&self) -> DataFrame {
        let per_tr: Vec<Vec<f32>> = self
            .records
            .iter()
            .map(TransducerRecord::output_ultrasound)
            .collect();
        crate::record::output_df("p[a.u.]", self.records.len(), &per_tr)
    }
}

#[cfg(test)]
mod tests {
    use autd3_rs_core::geometry::Point3;

    use crate::record::TransducerRecord;

    #[test]
    fn ultrasound_matches_voltage_length_and_is_finite() {
        let tr = TransducerRecord {
            pulse_width: vec![256, 256],
            phase: vec![0, 0],
            position: Point3::origin(),
        };
        let p = tr.output_ultrasound();
        assert_eq!(p.len(), 2 * 512);
        assert!(p.iter().all(|v| v.is_finite()));
    }
}
