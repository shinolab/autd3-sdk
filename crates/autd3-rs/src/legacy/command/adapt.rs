use crate::commands::{
    ChangeModulationBank, Clear, EmulateGpioIn, FixedCompletionTime, FixedUpdateRate, FociStm,
    ForceFan, GpioOut as NewGpioOut, Modulation, Nop, Pattern, PatternStm, PatternStmMode,
    SetGpioOut, SetOutputMask, SetPhaseCorrection, SetPulseWidthTable, SetSilencer, Synchronize,
};
use autd3_rs_core::value::{ModulationBank, PatternBank, TransitionMode};

use super::LegacyCommand;
use crate::legacy::datagram::LegacyDatagramBuilder;
use crate::legacy::op;
use crate::legacy::wire::{GainStmMode, GpioOut, Segment, TransitionMode as LegacyTransitionMode};

#[must_use]
pub(crate) const fn pattern_segment(bank: PatternBank) -> Segment {
    match bank {
        PatternBank::B0 => Segment::S0,
        PatternBank::B1 => Segment::S1,
    }
}

#[must_use]
pub(crate) const fn modulation_segment(bank: ModulationBank) -> Segment {
    match bank {
        ModulationBank::B0 => Segment::S0,
        ModulationBank::B1 => Segment::S1,
    }
}

#[must_use]
pub(crate) fn transition_mode(mode: TransitionMode, dc_offset_ns: i64) -> LegacyTransitionMode {
    if let TransitionMode::SysTime {
        margin: Some(margin),
        ..
    } = mode
    {
        tracing::warn!(
            ?margin,
            "legacy firmware uses a fixed 10 ms transition margin; the requested margin is ignored"
        );
    }
    match mode {
        TransitionMode::SyncIdx => LegacyTransitionMode::SyncIdx,
        TransitionMode::SysTime { time, .. } => {
            LegacyTransitionMode::SysTime(time.with_dc_offset(dc_offset_ns))
        }
        TransitionMode::Gpio(pin) => LegacyTransitionMode::Gpio(pin),
        TransitionMode::Ext => LegacyTransitionMode::Ext,
        TransitionMode::Immediate => LegacyTransitionMode::Immediate,
        TransitionMode::Later => LegacyTransitionMode::Later,
        other => {
            tracing::warn!(
                ?other,
                "legacy firmware has no equivalent transition mode; falling back to Immediate"
            );
            LegacyTransitionMode::Immediate
        }
    }
}

#[must_use]
const fn gain_stm_mode(mode: PatternStmMode) -> GainStmMode {
    match mode {
        PatternStmMode::PhaseIntensityFull => GainStmMode::PhaseIntensityFull,
        PatternStmMode::PhaseFull => GainStmMode::PhaseFull,
        PatternStmMode::PhaseHalf => GainStmMode::PhaseHalf,
    }
}

impl<'a> LegacyCommand<'a> for Nop {
    fn expand(self, builder: &mut LegacyDatagramBuilder<'a>) {
        builder.push_op(op::Nop::new());
    }
}

impl<'a> LegacyCommand<'a> for Clear {
    fn expand(self, builder: &mut LegacyDatagramBuilder<'a>) {
        builder.push_op(op::Clear::new());
    }
}

impl<'a> LegacyCommand<'a> for Synchronize {
    fn expand(self, builder: &mut LegacyDatagramBuilder<'a>) {
        builder.push_op(op::Sync::new());
    }
}

impl<'a> LegacyCommand<'a> for ForceFan {
    fn expand(self, builder: &mut LegacyDatagramBuilder<'a>) {
        builder.push_op(op::ForceFan::new(self.value));
    }
}

impl<'a> LegacyCommand<'a> for SetSilencer<FixedCompletionTime> {
    fn expand(self, builder: &mut LegacyDatagramBuilder<'a>) {
        builder.push_op(op::Silencer::new(op::SilencerConfig::FixedCompletionTime {
            intensity: self.config.intensity,
            phase: self.config.phase,
            strict: self.config.strict_mode,
        }));
    }
}

impl<'a> LegacyCommand<'a> for SetSilencer<FixedUpdateRate> {
    fn expand(self, builder: &mut LegacyDatagramBuilder<'a>) {
        builder.push_op(op::Silencer::new(op::SilencerConfig::FixedUpdateRate {
            intensity: self.config.intensity,
            phase: self.config.phase,
        }));
    }
}

impl<'a> LegacyCommand<'a> for Pattern<'a> {
    fn expand(self, builder: &mut LegacyDatagramBuilder<'a>) {
        builder.push_op(op::Gain::with_segment(
            self.emissions,
            pattern_segment(self.bank),
            !self.transition_mode.is_later(),
        ));
    }
}

impl<'a> LegacyCommand<'a> for Modulation<'a> {
    fn expand(self, builder: &mut LegacyDatagramBuilder<'a>) {
        let transition_mode = transition_mode(self.transition_mode, builder.dc_offset_ns());
        builder.push_op(op::Modulation::new(
            self.config,
            self.data,
            op::ModulationOption {
                segment: modulation_segment(self.bank),
                loop_behavior: self.loop_behavior,
                transition_mode,
            },
        ));
    }
}

impl<'a> LegacyCommand<'a> for ChangeModulationBank {
    fn expand(self, builder: &mut LegacyDatagramBuilder<'a>) {
        let transition_mode = transition_mode(self.transition_mode, builder.dc_offset_ns());
        builder.push_op(op::LegacyChangePatternBank::modulation(
            modulation_segment(self.bank),
            transition_mode,
        ));
    }
}

impl<'a, const N: usize> LegacyCommand<'a> for FociStm<'a, N> {
    fn expand(self, builder: &mut LegacyDatagramBuilder<'a>) {
        let config = self.config.into_sampling_config(self.points.len());
        let transition_mode = transition_mode(self.option.transition_mode, builder.dc_offset_ns());
        builder.push_op(op::FociStm::new(
            config,
            self.points,
            op::FociStmOption {
                segment: pattern_segment(self.option.bank),
                sound_speed: self.option.sound_speed,
                loop_behavior: self.option.loop_behavior,
                transition_mode,
            },
        ));
    }
}

impl<'a> LegacyCommand<'a> for PatternStm<'a> {
    fn expand(self, builder: &mut LegacyDatagramBuilder<'a>) {
        let config = self.config.into_sampling_config(self.patterns.len());
        let transition_mode = transition_mode(self.option.transition_mode, builder.dc_offset_ns());
        builder.push_op(op::GainStm::new(
            config,
            self.patterns,
            op::GainStmOption {
                mode: gain_stm_mode(self.option.mode),
                segment: pattern_segment(self.option.bank),
                loop_behavior: self.option.loop_behavior,
                transition_mode,
            },
        ));
    }
}

#[must_use]
fn gpio_out(output: NewGpioOut, dc_offset_ns: i64) -> GpioOut {
    match output {
        NewGpioOut::Off => GpioOut::Off,
        NewGpioOut::BaseSignal => GpioOut::BaseSignal,
        NewGpioOut::Thermo => GpioOut::Thermo,
        NewGpioOut::ForceFan => GpioOut::ForceFan,
        NewGpioOut::Sync => GpioOut::Sync,
        NewGpioOut::ModBank => GpioOut::ModSegment,
        NewGpioOut::ModIdx(idx) => GpioOut::ModIdx(idx),
        NewGpioOut::PatternBank => GpioOut::StmSegment,
        NewGpioOut::PatternIdx(idx) => GpioOut::StmIdx(idx),
        NewGpioOut::IsStmMode => GpioOut::IsStmMode,
        NewGpioOut::SysTimeEq(t) => GpioOut::SysTimeEq(t.with_dc_offset(dc_offset_ns)),
        NewGpioOut::SyncDiff => GpioOut::SyncDiff,
        NewGpioOut::PwmOut(tr) => GpioOut::PwmOut(tr),
        NewGpioOut::Direct(on) => GpioOut::Direct(on),
    }
}

impl<'a> LegacyCommand<'a> for SetOutputMask<'a> {
    fn expand(self, builder: &mut LegacyDatagramBuilder<'a>) {
        builder.push_op(op::SetOutputMask::new(self.masks, Segment::S0));
    }
}

impl<'a> LegacyCommand<'a> for SetPhaseCorrection<'a> {
    fn expand(self, builder: &mut LegacyDatagramBuilder<'a>) {
        builder.push_op(op::SetPhaseCorrection::new(self.phases));
    }
}

impl<'a> LegacyCommand<'a> for SetPulseWidthTable<'a> {
    fn expand(self, builder: &mut LegacyDatagramBuilder<'a>) {
        builder.push_op(op::SetPulseWidthTable::new(self.table));
    }
}

impl<'a> LegacyCommand<'a> for SetGpioOut {
    fn expand(self, builder: &mut LegacyDatagramBuilder<'a>) {
        let dc_offset_ns = builder.dc_offset_ns();
        builder.push_op(op::SetGpioOut::new(
            self.outputs.map(|out| gpio_out(out, dc_offset_ns)),
        ));
    }
}

impl<'a> LegacyCommand<'a> for EmulateGpioIn {
    fn expand(self, builder: &mut LegacyDatagramBuilder<'a>) {
        builder.push_op(op::EmulateGpioIn::new(self.values));
    }
}

#[cfg(test)]
mod tests {
    use autd3_rs_core::value::{DcSysTime, GpioIn};
    use core::time::Duration;

    use super::*;

    #[test]
    fn banks_map_onto_the_two_legacy_segments() {
        assert_eq!(pattern_segment(PatternBank::B0), Segment::S0);
        assert_eq!(pattern_segment(PatternBank::B1), Segment::S1);
        assert_eq!(modulation_segment(ModulationBank::B0), Segment::S0);
        assert_eq!(modulation_segment(ModulationBank::B1), Segment::S1);
    }

    #[test]
    fn transition_modes_map_one_to_one() {
        let time = DcSysTime::from_nanos(0x1234);
        assert_eq!(
            transition_mode(TransitionMode::SyncIdx, 0),
            LegacyTransitionMode::SyncIdx
        );
        assert_eq!(
            transition_mode(TransitionMode::SysTime { time, margin: None }, 0),
            LegacyTransitionMode::SysTime(time)
        );
        assert_eq!(
            transition_mode(TransitionMode::Gpio(GpioIn::I2), 0),
            LegacyTransitionMode::Gpio(GpioIn::I2)
        );
        assert_eq!(
            transition_mode(TransitionMode::Ext, 0),
            LegacyTransitionMode::Ext
        );
        assert_eq!(
            transition_mode(TransitionMode::Immediate, 0),
            LegacyTransitionMode::Immediate
        );
        assert_eq!(
            transition_mode(TransitionMode::Later, 0),
            LegacyTransitionMode::Later
        );
    }

    #[test]
    fn a_requested_sys_time_margin_is_dropped() {
        let time = DcSysTime::from_nanos(0x1234);
        assert_eq!(
            transition_mode(
                TransitionMode::SysTime {
                    time,
                    margin: Some(Duration::from_millis(5)),
                },
                0
            ),
            LegacyTransitionMode::SysTime(time),
            "legacy firmware hard-codes a 10 ms margin"
        );
    }

    #[test]
    fn sys_time_is_retimed_onto_the_bus_clock() {
        let time = DcSysTime::from_nanos(1_000_000);
        assert_eq!(
            transition_mode(TransitionMode::SysTime { time, margin: None }, 500),
            LegacyTransitionMode::SysTime(DcSysTime::from_nanos(1_000_500))
        );
        assert_eq!(
            transition_mode(TransitionMode::SysTime { time, margin: None }, -500),
            LegacyTransitionMode::SysTime(DcSysTime::from_nanos(999_500))
        );
        assert_eq!(
            transition_mode(TransitionMode::SyncIdx, 500),
            LegacyTransitionMode::SyncIdx,
            "modes without a time are untouched"
        );
    }

    #[test]
    fn sys_time_eq_is_retimed_before_it_is_scaled() {
        let ns = 3125u64 * 4096;
        assert_eq!(
            gpio_out(NewGpioOut::SysTimeEq(DcSysTime::from_nanos(ns)), 3125),
            GpioOut::SysTimeEq(DcSysTime::from_nanos(ns + 3125))
        );
        assert_eq!(
            gpio_out(NewGpioOut::SysTimeEq(DcSysTime::from_nanos(ns)), 3125).encode(),
            GpioOut::SysTimeEq(DcSysTime::from_nanos(ns + 3125)).encode()
        );
        assert_eq!(
            gpio_out(NewGpioOut::BaseSignal, 3125),
            GpioOut::BaseSignal,
            "outputs without a time are untouched"
        );
    }

    #[test]
    fn stm_modes_map_one_to_one() {
        assert_eq!(
            gain_stm_mode(PatternStmMode::PhaseIntensityFull),
            GainStmMode::PhaseIntensityFull
        );
        assert_eq!(
            gain_stm_mode(PatternStmMode::PhaseFull),
            GainStmMode::PhaseFull
        );
        assert_eq!(
            gain_stm_mode(PatternStmMode::PhaseHalf),
            GainStmMode::PhaseHalf
        );
    }
}
