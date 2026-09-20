use autd3_cpu_wire::layout::{FUSED_EMISSION_TYPE_RAW_SOA, PATTERN_RAW_DATA_LEN};
use autd3_cpu_wire::params::EMISSION_TYPE_FOCI;
use autd3_cpu_wire::payload::WritePatternFusedPayload;
use zerocopy::FromBytes;
use zerocopy::little_endian::{U16, U32, U64};

use crate::Velocity;
use crate::error::{Error, PayloadError};
use crate::geometry::Device;
use crate::mirror::FirmwareState;
use crate::params::{BUFFER_SIZE_MIN, FOCUS_WORDS, MAX_FOCI_TOTAL, NUM_FOCI_MAX};
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::value::{
    ControlPoints, LoopBehavior, PatternBank, Phase, SamplingConfig, TransitionMode,
};

use super::write_pattern_buffer::{PatternIntensity, encode_raw_slot};
use super::{Distribution, Operation, check_index_advance};

pub(crate) const PATTERN_FUSED_HEADER_BYTES: usize =
    core::mem::size_of::<WritePatternFusedPayload>();
const PATTERN_FUSED_MAX_DATA_LEN: usize = PAYLOAD_BYTES - PATTERN_FUSED_HEADER_BYTES;
pub(crate) const PATTERN_FUSED_MAX_FOCI_PER_FRAME: usize =
    PATTERN_FUSED_MAX_DATA_LEN / (FOCUS_WORDS * 2);

const _: () = assert!(PATTERN_RAW_DATA_LEN <= PATTERN_FUSED_MAX_DATA_LEN);

#[derive(Clone, Copy, Debug)]
pub struct WritePatternFused<'a> {
    pub bank: PatternBank,
    pub phases: &'a [Vec<Phase>],
    pub intensities: PatternIntensity<'a>,
    pub config: SamplingConfig,
    pub loop_behavior: LoopBehavior,
    pub transition_mode: TransitionMode,
}

impl<'a> WritePatternFused<'a> {
    #[must_use]
    pub fn new(
        bank: PatternBank,
        phases: &'a [Vec<Phase>],
        intensities: impl Into<PatternIntensity<'a>>,
        config: SamplingConfig,
        loop_behavior: LoopBehavior,
        transition_mode: TransitionMode,
    ) -> Self {
        Self {
            bank,
            phases,
            intensities: intensities.into(),
            config,
            loop_behavior,
            transition_mode,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct WriteFociStmFused<'a, const N: usize> {
    pub bank: PatternBank,
    pub points: &'a [ControlPoints<N>],
    pub config: SamplingConfig,
    pub sound_speed: Velocity,
    pub loop_behavior: LoopBehavior,
    pub transition_mode: TransitionMode,
}

fn reflect_fused(
    config: SamplingConfig,
    bank: PatternBank,
    loop_behavior: LoopBehavior,
    transition_mode: TransitionMode,
    device: usize,
    state: &mut FirmwareState,
) -> Result<(), Error> {
    let divider = config.divide()?;
    let bank = bank.as_u8();
    state.silencer.check_pattern_div(device, divider)?;
    state.silencer.note_pattern_div(bank, divider);
    state.transition.note_pattern_loop(bank, loop_behavior);

    state.silencer.check_pattern_bank(device, bank)?;
    state
        .transition
        .check_pattern_bank(device, bank, transition_mode)?;
    state.silencer.note_pattern_bank(bank);
    Ok(())
}

impl crate::sealed::Sealed for WritePatternFused<'_> {}

impl Operation for WritePatternFused<'_> {
    fn apply_dc_offset(&mut self, offset_ns: i64) {
        self.transition_mode = self.transition_mode.with_dc_offset(offset_ns);
    }

    fn distribution(&self) -> Distribution {
        Distribution::PerDevice
    }

    fn encode(&self, device: &Device, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Cmd, Error> {
        check_index_advance(1, self.loop_behavior)?;
        let divider = self.config.divide()?;
        let margin_ns = self.transition_mode.margin_ns()?;

        let (h, rest) = WritePatternFusedPayload::mut_from_prefix(&mut out[..]).unwrap();
        encode_raw_slot(self.phases, self.intensities, device, rest)?;
        *h = WritePatternFusedPayload {
            bank: self.bank.as_u8(),
            emission_type: FUSED_EMISSION_TYPE_RAW_SOA,
            divider: U16::new(divider),
            size: U32::new(1),
            num_foci: 0,
            transition_mode: self.transition_mode.try_as_u8()?,
            sound_speed: U16::new(0),
            rep: U16::new(self.loop_behavior.rep()),
            data_len: U16::new(
                u16::try_from(PATTERN_RAW_DATA_LEN).expect("bounded by frame capacity"),
            ),
            transition_value: U64::new(self.transition_mode.value()),
            margin_ns: U32::new(margin_ns),
            reserved: U32::new(0),
        };
        Ok(Cmd::WritePatternFused)
    }

    fn reflect(&self, device: usize, state: &mut FirmwareState) -> Result<(), Error> {
        reflect_fused(
            self.config,
            self.bank,
            self.loop_behavior,
            self.transition_mode,
            device,
            state,
        )
    }
}

impl<const N: usize> WriteFociStmFused<'_, N> {
    #[must_use]
    pub fn fits_single_frame(points: usize) -> bool {
        points > 0 && points * N <= PATTERN_FUSED_MAX_FOCI_PER_FRAME
    }
}

impl<const N: usize> crate::sealed::Sealed for WriteFociStmFused<'_, N> {}

impl<const N: usize> Operation for WriteFociStmFused<'_, N> {
    fn apply_dc_offset(&mut self, offset_ns: i64) {
        self.transition_mode = self.transition_mode.with_dc_offset(offset_ns);
    }

    fn distribution(&self) -> Distribution {
        Distribution::PerDevice
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn encode(&self, device: &Device, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Cmd, Error> {
        let size = self.points.len();
        if size < BUFFER_SIZE_MIN {
            return Err(PayloadError::PatternSizeTooSmall {
                size,
                min: BUFFER_SIZE_MIN,
            }
            .into());
        }
        let num_foci = u8::try_from(N).unwrap_or(u8::MAX);
        if num_foci == 0 || num_foci > NUM_FOCI_MAX {
            return Err(PayloadError::NumFociOutOfRange {
                num_foci,
                max: NUM_FOCI_MAX,
            }
            .into());
        }
        let total = size * N;
        if total > PATTERN_FUSED_MAX_FOCI_PER_FRAME {
            return Err(PayloadError::FociWriteExceedsCapacity {
                offset: 0,
                end: total,
                capacity: PATTERN_FUSED_MAX_FOCI_PER_FRAME,
            }
            .into());
        }
        if size > MAX_FOCI_TOTAL / usize::from(num_foci) {
            return Err(PayloadError::StmFociExceedCapacity {
                size,
                num_foci,
                capacity: MAX_FOCI_TOTAL,
            }
            .into());
        }
        let divider = self.config.divide()?;
        let sound_speed = (self.sound_speed.m_s() * 64.0).round() as u16;
        if sound_speed == 0 {
            return Err(PayloadError::SoundSpeedZero.into());
        }
        let margin_ns = self.transition_mode.margin_ns()?;
        let data_len = u16::try_from(total * FOCUS_WORDS * 2).expect("bounded by frame");

        let (h, rest) = WritePatternFusedPayload::mut_from_prefix(&mut out[..]).unwrap();
        *h = WritePatternFusedPayload {
            bank: self.bank.as_u8(),
            emission_type: EMISSION_TYPE_FOCI,
            divider: U16::new(divider),
            size: U32::new(u32::try_from(size).expect("bounded by capacity checks")),
            num_foci,
            transition_mode: self.transition_mode.try_as_u8()?,
            sound_speed: U16::new(sound_speed),
            rep: U16::new(self.loop_behavior.rep()),
            data_len: U16::new(data_len),
            transition_value: U64::new(self.transition_mode.value()),
            margin_ns: U32::new(margin_ns),
            reserved: U32::new(0),
        };
        for (dst, k) in rest.as_chunks_mut::<8>().0.iter_mut().zip(0..total) {
            let focus = self.points[k / N].focus(device, k % N);
            *dst = focus.encode()?.to_le_bytes();
        }
        Ok(Cmd::WritePatternFused)
    }

    fn reflect(&self, device: usize, state: &mut FirmwareState) -> Result<(), Error> {
        reflect_fused(
            self.config,
            self.bank,
            self.loop_behavior,
            self.transition_mode,
            device,
            state,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Autd3, Point3};
    use crate::test_utils::test_device;
    use crate::value::{ControlPoint, Focus, Intensity, Phase};
    use core::num::NonZeroU16;

    #[test]
    fn fused_pattern_lays_out_header_and_data() {
        let n = Autd3::NUM_TRANSDUCERS;
        let phases = [(0..n)
            .map(|i| Phase(u8::try_from(i % 251).unwrap()))
            .collect::<Vec<_>>()];
        let intensities = [(0..n)
            .map(|i| Intensity(u8::try_from((i * 3) % 256).unwrap()))
            .collect::<Vec<_>>()];
        let op = WritePatternFused::new(
            PatternBank::B1,
            &phases,
            &intensities,
            SamplingConfig::new(NonZeroU16::new(7).unwrap()),
            LoopBehavior::Infinite,
            TransitionMode::Immediate,
        );

        let mut out = [0u8; PAYLOAD_BYTES];
        let cmd = op.encode(&test_device(0), &mut out).unwrap();

        assert_eq!(cmd, Cmd::WritePatternFused);
        assert_eq!(out[0], 1, "bank B1");
        assert_eq!(out[1], FUSED_EMISSION_TYPE_RAW_SOA);
        assert_eq!(&out[2..4], &7u16.to_le_bytes(), "divider");
        assert_eq!(&out[4..8], &1u32.to_le_bytes(), "size = 1 index");
        assert_eq!(out[8], 0, "num_foci unused for raw");
        assert_eq!(out[9], 0xFF, "IMMEDIATE");
        assert_eq!(&out[12..14], &0xFFFFu16.to_le_bytes(), "infinite rep");
        assert_eq!(&out[14..16], &498u16.to_le_bytes(), "data_len");
        for i in 0..n {
            assert_eq!(out[PATTERN_FUSED_HEADER_BYTES + i], phases[0][i].0);
            assert_eq!(out[PATTERN_FUSED_HEADER_BYTES + n + i], intensities[0][i].0);
        }
    }

    #[test]
    fn fused_foci_lays_out_header_and_data() {
        let points = [
            ControlPoints::new(
                [ControlPoint::new(Point3::new(0.0, 0.0, 150.0), Phase::ZERO)],
                Intensity(0xAA),
            ),
            ControlPoints::new(
                [ControlPoint::new(Point3::new(0.0, 0.0, 200.0), Phase::ZERO)],
                Intensity(0xBB),
            ),
        ];
        let op = WriteFociStmFused {
            bank: PatternBank::B0,
            points: &points,
            config: SamplingConfig::FREQ_4K,
            sound_speed: Velocity::from_m_s(340.0),
            loop_behavior: LoopBehavior::Infinite,
            transition_mode: TransitionMode::Immediate,
        };

        let mut out = [0u8; PAYLOAD_BYTES];
        let cmd = op.encode(&test_device(0), &mut out).unwrap();

        assert_eq!(cmd, Cmd::WritePatternFused);
        assert_eq!(out[1], EMISSION_TYPE_FOCI);
        assert_eq!(&out[4..8], &2u32.to_le_bytes(), "size = sample count");
        assert_eq!(out[8], 1, "num_foci = N");
        assert_eq!(&out[10..12], &21760u16.to_le_bytes(), "340 m/s * 64");
        assert_eq!(&out[14..16], &16u16.to_le_bytes(), "data_len = 2 foci");

        let expected = Focus {
            x: 0,
            y: 0,
            z: 6000,
            intensity_or_offset: 0xAA,
        }
        .encode()
        .unwrap();
        let first = u64::from_le_bytes(
            out[PATTERN_FUSED_HEADER_BYTES..PATTERN_FUSED_HEADER_BYTES + 8]
                .try_into()
                .unwrap(),
        );
        assert_eq!(first, expected);
    }

    #[test]
    fn fused_foci_rejects_more_than_one_frame() {
        let points: Vec<ControlPoints<1>> = (0..=PATTERN_FUSED_MAX_FOCI_PER_FRAME)
            .map(|i| ControlPoints::from(Point3::new(0.0, 0.0, i as f32 * 0.1)))
            .collect();
        let op = WriteFociStmFused {
            bank: PatternBank::B0,
            points: &points,
            config: SamplingConfig::FREQ_4K,
            sound_speed: Velocity::from_m_s(340.0),
            loop_behavior: LoopBehavior::Infinite,
            transition_mode: TransitionMode::Immediate,
        };
        let mut out = [0u8; PAYLOAD_BYTES];
        assert!(matches!(
            op.encode(&test_device(0), &mut out),
            Err(Error::InvalidPayload(_))
        ));

        assert!(!WriteFociStmFused::<1>::fits_single_frame(
            PATTERN_FUSED_MAX_FOCI_PER_FRAME + 1
        ));
        assert!(WriteFociStmFused::<1>::fits_single_frame(
            PATTERN_FUSED_MAX_FOCI_PER_FRAME
        ));
    }

    #[test]
    fn fused_pattern_encodes_transition_value_and_margin() {
        use crate::value::DcSysTime;
        use core::time::Duration;

        let points = [
            ControlPoints::from(Point3::new(0.0, 0.0, 150.0)),
            ControlPoints::from(Point3::new(0.0, 0.0, 200.0)),
        ];
        let op = WriteFociStmFused {
            bank: PatternBank::B0,
            points: &points,
            config: SamplingConfig::FREQ_4K,
            sound_speed: Velocity::from_m_s(340.0),
            loop_behavior: LoopBehavior::Finite(NonZeroU16::new(8).unwrap()),
            transition_mode: TransitionMode::SysTime {
                time: DcSysTime::from_nanos(0xDEAD_BEEF),
                margin: Some(Duration::from_millis(1)),
            },
        };
        let mut out = [0u8; PAYLOAD_BYTES];
        op.encode(&test_device(0), &mut out).unwrap();

        assert_eq!(out[9], 0x01, "SYS_TIME");
        assert_eq!(&out[12..14], &7u16.to_le_bytes(), "Finite(8) => rep 7");
        assert_eq!(&out[16..24], &0xDEAD_BEEFu64.to_le_bytes());
        assert_eq!(&out[24..28], &1_000_000u32.to_le_bytes());
    }

    #[test]
    fn fused_pattern_rejects_device_out_of_range() {
        let phases = [vec![Phase::ZERO; Autd3::NUM_TRANSDUCERS]];
        let intensities = [vec![Intensity::MAX; Autd3::NUM_TRANSDUCERS]];
        let op = WritePatternFused::new(
            PatternBank::B0,
            &phases,
            &intensities,
            SamplingConfig::FREQ_4K,
            LoopBehavior::Infinite,
            TransitionMode::Immediate,
        );
        let mut out = [0u8; PAYLOAD_BYTES];
        assert!(op.encode(&test_device(0), &mut out).is_ok());
        assert!(matches!(
            op.encode(&test_device(1), &mut out),
            Err(Error::InvalidPayload(_))
        ));
    }
}
