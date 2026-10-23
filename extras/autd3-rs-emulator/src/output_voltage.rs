use autd3_rs_firmware_emulator::pwm_levels;

use crate::raw::RawFrame;
use crate::record::{
    OUTPUT_VOLTAGE, Record, TransducerRecord, ULTRASOUND_PERIOD_COUNT, output_frame,
};

impl TransducerRecord {
    fn voltage_frame(pw: u16, phase: u8, out: &mut Vec<f32>) {
        out.extend(pwm_levels(pw, phase).map(|high| {
            if high {
                OUTPUT_VOLTAGE
            } else {
                -OUTPUT_VOLTAGE
            }
        }));
    }

    pub(crate) fn output_voltage_within(&self, start: usize, n: usize) -> Option<Vec<f32>> {
        if start + n > self.pulse_width.len() {
            return None;
        }
        let mut out = Vec::with_capacity(n * ULTRASOUND_PERIOD_COUNT);
        for (pw, phase) in self.pulse_width[start..][..n]
            .iter()
            .zip(self.phase[start..][..n].iter())
        {
            Self::voltage_frame(*pw, *phase, &mut out);
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
    pub fn output_voltage(&self) -> RawFrame {
        let per_tr: Vec<Vec<f32>> = self
            .records
            .iter()
            .map(TransducerRecord::output_voltage)
            .collect();
        output_frame("voltage[V]", &per_tr)
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
