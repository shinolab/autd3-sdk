mod modulation;
pub(crate) mod operation;
mod pattern;
pub(crate) mod stm;

pub use modulation::Modulation;
pub use pattern::Pattern;

pub use operation::{
    ActivateModulationBank, ActivatePatternBank, Clear, ConfigFociStm, ConfigModulation,
    ConfigPattern, CpuConfig, Distribution, EmulateGpioIn, FixedCompletionTime, FixedUpdateRate,
    ForceFan, FpgaBusWait, FpgaConfig, GpioOut, Nop, Operation, PWE_TABLE_SIZE, PatternIntensity,
    PhaseDepth, PtpConfig, ReleaseFailsafe, SetCpuConfig, SetFpgaConfig, SetGpioOut, SetOutputMask,
    SetPhaseCorrection, SetPulseWidthTable, SetSilencer, SilencerConfig, StmIntensity, Synchronize,
    WriteFociBuffer, WriteModulationBuffer, WritePatternBuffer, WritePatternPhase,
};
#[doc(hidden)]
pub use operation::{
    FpgaMemoryRead, FpgaMemoryWrite, FpgaUpdateActivate, FpgaUpdateBegin, FpgaUpdateChunk,
    FpgaUpdateCommit, Reboot, UpdateBegin, UpdateChunk, UpdateCommit, UpdateConfirm,
};
pub use stm::{FociStm, FociStmOption, PatternStm, PatternStmOption};

pub use crate::datagram::{Each, Expansion, each};
use crate::error::Error;

pub trait Command {
    fn expand(self, expansion: &mut Expansion<'_>) -> Result<(), Error>;

    #[must_use]
    fn boxed<'a>(self) -> BoxedCommand<'a>
    where
        Self: Sized + Send + 'a,
    {
        BoxedCommand(Box::new(self))
    }
}

impl<O: Operation> Command for O {
    fn expand(self, expansion: &mut Expansion<'_>) -> Result<(), Error> {
        expansion.push_op(&self)
    }
}

trait DynCommand {
    fn expand_boxed(self: Box<Self>, expansion: &mut Expansion<'_>) -> Result<(), Error>;
}

impl<C: Command> DynCommand for C {
    fn expand_boxed(self: Box<Self>, expansion: &mut Expansion<'_>) -> Result<(), Error> {
        (*self).expand(expansion)
    }
}

pub struct BoxedCommand<'a>(Box<dyn DynCommand + Send + 'a>);

impl Command for BoxedCommand<'_> {
    fn expand(self, expansion: &mut Expansion<'_>) -> Result<(), Error> {
        self.0.expand_boxed(expansion)
    }
}

impl<C: Command> Command for Vec<C> {
    fn expand(self, expansion: &mut Expansion<'_>) -> Result<(), Error> {
        for cmd in self {
            expansion.push(cmd)?;
        }
        Ok(())
    }
}

impl<C: Command, const N: usize> Command for [C; N] {
    fn expand(self, expansion: &mut Expansion<'_>) -> Result<(), Error> {
        for cmd in self {
            expansion.push(cmd)?;
        }
        Ok(())
    }
}

impl<C: Command + Clone> Command for &[C] {
    fn expand(self, expansion: &mut Expansion<'_>) -> Result<(), Error> {
        for cmd in self {
            expansion.push(cmd.clone())?;
        }
        Ok(())
    }
}

macro_rules! impl_command_for_tuple {
    ($($name:ident),+) => {
        impl<$($name: Command),+> Command for ($($name,)+) {
            #[allow(non_snake_case)]
            fn expand(self, expansion: &mut Expansion<'_>) -> Result<(), Error> {
                let ($($name,)+) = self;
                $(expansion.push($name)?;)+
                Ok(())
            }
        }
    };
}

impl_command_for_tuple!(A, B);
impl_command_for_tuple!(A, B, C);
impl_command_for_tuple!(A, B, C, D);
impl_command_for_tuple!(A, B, C, D, E);
impl_command_for_tuple!(A, B, C, D, E, F);
impl_command_for_tuple!(A, B, C, D, E, F, G);
impl_command_for_tuple!(A, B, C, D, E, F, G, H);

#[cfg(test)]
mod tests {
    use crate::datagram::{Frames, each};
    use crate::error::Error;
    use crate::test_utils::{FailAt, Marker, build, payload, test_geometry};

    fn markers(frames: &Frames) -> Vec<u8> {
        (0..frames.len())
            .map(|i| payload(frames, i, 0)[0])
            .collect()
    }

    #[test]
    fn a_tuple_expands_its_commands_in_order() {
        let frames = build(
            2,
            (
                Marker(0),
                each(|_| Some(Marker(1))),
                (Marker(2), Marker(3)),
                Marker(4),
                Marker(5),
                Marker(6),
                Marker(7),
            ),
        )
        .unwrap();

        assert_eq!(markers(&frames), [0, 1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn a_vec_an_array_and_a_slice_expand_their_commands_in_order() {
        let commands = vec![Marker(0), Marker(1), Marker(2)];

        assert_eq!(markers(&build(1, &commands[..]).unwrap()), [0, 1, 2]);
        assert_eq!(markers(&build(1, commands).unwrap()), [0, 1, 2]);
        assert_eq!(markers(&build(1, [Marker(3), Marker(4)]).unwrap()), [3, 4]);
        assert!(build(1, Vec::<Marker>::new()).unwrap().is_empty());
    }

    #[test]
    fn a_failing_element_leaves_no_frame_behind() {
        let mut frames = Frames::default();
        let result = frames.encode_into(&test_geometry(2), vec![(Marker(0), FailAt(1))]);

        assert!(matches!(result, Err(Error::InvalidPayload(_))));
        assert!(frames.is_empty());
    }
}
