#![allow(clippy::cast_possible_truncation)]

#[cfg(feature = "polars")]
use polars::frame::DataFrame;

use crate::record::{OUTPUT_VOLTAGE, Record, TransducerRecord, ULTRASOUND_PERIOD_COUNT};

impl TransducerRecord {
    fn voltage_frame(pw: u16, phase: u16, out: &mut Vec<f32>) {
        const T: u16 = ULTRASOUND_PERIOD_COUNT as u16;
        let rise = ((T + phase * 2) - pw / 2) % T;
        let fall = (phase * 2 + pw / 2 + (pw & 0x01)) % T;
        for i in 0..T {
            let high = if rise <= fall {
                rise <= i && i < fall
            } else {
                i < fall || rise <= i
            };
            out.push(if high {
                OUTPUT_VOLTAGE
            } else {
                -OUTPUT_VOLTAGE
            });
        }
    }

    pub(crate) fn output_voltage_within(&self, start: usize, n: usize) -> Option<Vec<f32>> {
        if start + n > self.pulse_width.len() {
            return None;
        }
        let mut out = Vec::with_capacity(n * ULTRASOUND_PERIOD_COUNT);
        for (pw, phase) in self.pulse_width[start..start + n]
            .iter()
            .zip(self.phase[start..start + n].iter())
        {
            Self::voltage_frame(*pw, u16::from(*phase), &mut out);
        }
        Some(out)
    }

    pub(crate) fn output_voltage(&self) -> Vec<f32> {
        self.output_voltage_within(0, self.pulse_width.len())
            .unwrap_or_default()
    }
}

impl Record {
    #[must_use]
    pub fn output_voltage_of(&self, transducer: usize) -> Vec<f32> {
        self.records[transducer].output_voltage()
    }

    #[cfg(feature = "polars")]
    #[must_use]
    pub fn output_voltage(&self) -> DataFrame {
        let per_tr: Vec<Vec<f32>> = self
            .records
            .iter()
            .map(TransducerRecord::output_voltage)
            .collect();
        crate::record::output_df("voltage[V]", self.records.len(), &per_tr)
    }
}

#[cfg(test)]
mod tests {
    use autd3_rs_core::geometry::Point3;

    use crate::record::TransducerRecord;

    #[test]
    fn square_wave_rise_fall() {
        let tr = TransducerRecord {
            pulse_width: vec![256],
            phase: vec![0],
            position: Point3::origin(),
        };
        let v = tr.output_voltage();
        assert_eq!(v.len(), 512);
        assert!(v[0] > 0.0);
        assert!(v[200] < 0.0);
        assert!(v[400] > 0.0);
    }
}
