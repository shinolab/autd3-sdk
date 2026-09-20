use autd3_rs_core::geometry::Device;
use autd3_rs_core::value::Phase;
use zerocopy::{Immutable, IntoBytes};

use super::LegacyOperation;
use crate::commands::PatternIntensity;
use crate::legacy::error::{LegacyError, PayloadError};
use crate::legacy::wire::{Segment, Tag, params::GAIN_FLAG_UPDATE};

#[repr(C)]
#[derive(Clone, Copy, IntoBytes, Immutable)]
struct GainHead {
    tag: u8,
    segment: u8,
    flag: u8,
    _pad: u8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gain<'a> {
    phases: &'a [Vec<Phase>],
    intensities: PatternIntensity<'a>,
    segment: Segment,
    transition: bool,
    done: bool,
}

impl<'a> Gain<'a> {
    #[must_use]
    pub fn new(phases: &'a [Vec<Phase>], intensities: impl Into<PatternIntensity<'a>>) -> Self {
        Self {
            phases,
            intensities: intensities.into(),
            segment: Segment::S0,
            transition: true,
            done: false,
        }
    }

    #[must_use]
    pub fn with_segment(
        phases: &'a [Vec<Phase>],
        intensities: impl Into<PatternIntensity<'a>>,
        segment: Segment,
        transition: bool,
    ) -> Self {
        Self {
            phases,
            intensities: intensities.into(),
            segment,
            transition,
            done: false,
        }
    }
}

pub(super) fn slot_for<'a, T>(
    data: &'a [Vec<T>],
    device: &Device,
) -> Result<&'a [T], PayloadError> {
    let slot = data
        .get(device.idx())
        .ok_or(PayloadError::EmissionDeviceCountMismatch {
            expected: device.idx() + 1,
            got: data.len(),
        })?;
    if slot.len() != device.num_transducers() {
        return Err(PayloadError::EmissionTransducerCountMismatch {
            device: device.idx(),
            expected: device.num_transducers(),
            got: slot.len(),
        });
    }
    Ok(slot)
}

pub(super) fn write_emissions(
    tx: &mut [u8],
    phases: &[Phase],
    intensities: PatternIntensity<'_>,
    device: &Device,
) -> Result<(), PayloadError> {
    match intensities {
        PatternIntensity::Uniform(intensity) => {
            for (dst, phase) in tx.as_chunks_mut::<2>().0.iter_mut().zip(phases) {
                *dst = [phase.0, intensity.0];
            }
        }
        PatternIntensity::PerDevice(intensities) => {
            for (dst, (phase, intensity)) in tx
                .as_chunks_mut::<2>()
                .0
                .iter_mut()
                .zip(phases.iter().zip(slot_for(intensities, device)?))
            {
                *dst = [phase.0, intensity.0];
            }
        }
    }
    Ok(())
}

impl LegacyOperation for Gain<'_> {
    fn required_size(&self, device: &Device) -> usize {
        size_of::<GainHead>() + device.num_transducers() * 2
    }

    fn pack(&mut self, device: &Device, tx: &mut [u8]) -> Result<usize, LegacyError> {
        let phases = slot_for(self.phases, device)?;
        let head = GainHead {
            tag: Tag::Gain.as_u8(),
            segment: self.segment.as_u8(),
            flag: if self.transition { GAIN_FLAG_UPDATE } else { 0 },
            _pad: 0,
        };
        tx[..size_of::<GainHead>()].copy_from_slice(head.as_bytes());
        write_emissions(
            &mut tx[size_of::<GainHead>()..],
            phases,
            self.intensities,
            device,
        )?;
        self.done = true;
        Ok(self.required_size(device))
    }

    fn is_done(&self) -> bool {
        self.done
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use autd3_rs_core::geometry::{Autd3, Geometry};
    use autd3_rs_core::value::Intensity;

    fn geometry(n: usize) -> Geometry {
        Geometry::new((0..n).map(|_| Autd3::default()).collect())
    }

    #[allow(clippy::cast_possible_truncation)]
    fn ramp_phases(n: usize, base: u8) -> Vec<Phase> {
        (0..n).map(|i| Phase(base.wrapping_add(i as u8))).collect()
    }

    #[allow(clippy::cast_possible_truncation)]
    fn ramp_intensities(n: usize, base: u8) -> Vec<Intensity> {
        (0..n)
            .map(|i| Intensity(base.wrapping_sub(i as u8)))
            .collect()
    }

    #[test]
    fn gain_writes_head_then_phase_intensity_pairs() {
        let geo = geometry(1);
        let n = geo[0].num_transducers();
        let phases = vec![ramp_phases(n, 0x10)];
        let intensities = vec![ramp_intensities(n, 0x10)];

        let mut op = Gain::new(&phases, &intensities);
        assert_eq!(op.required_size(&geo[0]), 4 + 2 * n);

        let mut tx = vec![0u8; 4 + 2 * n];
        assert_eq!(op.pack(&geo[0], &mut tx).unwrap(), 4 + 2 * n);
        assert!(op.is_done());

        assert_eq!(tx[0], Tag::Gain.as_u8());
        assert_eq!(tx[1], Segment::S0.as_u8());
        assert_eq!(tx[2], GAIN_FLAG_UPDATE);
        assert_eq!(tx[3], 0);
        for (i, chunk) in tx[4..].as_chunks::<2>().0.iter().enumerate() {
            assert_eq!(chunk[0], phases[0][i].0);
            assert_eq!(chunk[1], intensities[0][i].0);
        }
    }

    #[test]
    fn gain_without_transition_clears_the_update_flag() {
        let geo = geometry(1);
        let n = geo[0].num_transducers();
        let phases = vec![ramp_phases(n, 0)];
        let intensities = vec![ramp_intensities(n, 0)];
        let mut op = Gain::with_segment(&phases, &intensities, Segment::S1, false);
        let mut tx = vec![0u8; op.required_size(&geo[0])];
        op.pack(&geo[0], &mut tx).unwrap();
        assert_eq!(tx[1], Segment::S1.as_u8());
        assert_eq!(tx[2], 0);
    }

    #[test]
    fn gain_uses_the_slot_matching_the_device_index() {
        let geo = geometry(2);
        let n = geo[0].num_transducers();
        let phases = vec![ramp_phases(n, 0x00), ramp_phases(n, 0x80)];
        let intensities = vec![ramp_intensities(n, 0x00), ramp_intensities(n, 0x80)];
        for device in &geo {
            let mut op = Gain::new(&phases, &intensities);
            let mut tx = vec![0u8; op.required_size(device)];
            op.pack(device, &mut tx).unwrap();
            assert_eq!(tx[4], phases[device.idx()][0].0);
        }
    }

    #[test]
    fn gain_rejects_a_buffer_with_the_wrong_shape() {
        let geo = geometry(2);
        let n = geo[0].num_transducers();

        let short = vec![ramp_phases(n, 0)];
        let full = vec![ramp_intensities(n, 0), ramp_intensities(n, 0)];
        let mut tx = vec![0u8; 4 + 2 * n];
        let err = Gain::new(&short, &full).pack(&geo[1], &mut tx).unwrap_err();
        assert!(matches!(
            err,
            LegacyError::InvalidPayload(PayloadError::EmissionDeviceCountMismatch { .. })
        ));

        let phases = vec![ramp_phases(n, 0), ramp_phases(n, 0)];
        let ragged = vec![ramp_intensities(n - 1, 0), ramp_intensities(n, 0)];
        let err = Gain::new(&phases, &ragged)
            .pack(&geo[0], &mut tx)
            .unwrap_err();
        assert!(matches!(
            err,
            LegacyError::InvalidPayload(PayloadError::EmissionTransducerCountMismatch {
                device: 0,
                ..
            })
        ));
    }
}
