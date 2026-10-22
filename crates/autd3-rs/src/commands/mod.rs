mod modulation;
pub(crate) mod operation;
mod pattern;
pub(crate) mod stm;

pub use modulation::Modulation;
pub use pattern::Pattern;

pub use operation::{
    ActivateModulationBank, ActivatePatternBank, Clear, ConfigFociStm, ConfigModulation,
    ConfigPattern, CpuConfig, Distribution, EmulateGpioIn, FixedCompletionTime, FixedUpdateRate,
    ForceFan, FpgaBusWait, FpgaConfig, GpioOut, MaxFreqPpb, Nop, Operation, PWE_TABLE_SIZE,
    PathDelayFilterShift, PatternIntensity, PhaseDepth, PtpConfig, ReleaseFailsafe, SetCpuConfig,
    SetFpgaConfig, SetGpioOut, SetOutputMask, SetPhaseCorrection, SetPulseWidthTable, SetSilencer,
    SilencerConfig, StmIntensity, Synchronize, WriteFociBuffer, WriteModulationBuffer,
    WritePatternBuffer, WritePatternPhase,
};
pub use stm::{FociStm, FociStmOption, PatternStm, PatternStmOption};

pub use crate::datagram::{Each, Expansion, each};
use crate::error::Error;

pub trait Command {
    fn expand(self, expansion: &mut Expansion<'_>) -> Result<(), Error>;

    #[must_use]
    fn boxed<'a>(self) -> BoxedCommand<'a>
    where
        Self: Sized + 'a,
    {
        BoxedCommand(Box::new(self))
    }
}

impl<O: Operation> Command for O {
    fn expand(self, expansion: &mut Expansion<'_>) -> Result<(), Error> {
        expansion.push_op(&self)?;
        Ok(())
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

pub struct BoxedCommand<'a>(Box<dyn DynCommand + 'a>);

impl Command for BoxedCommand<'_> {
    fn expand(self, expansion: &mut Expansion<'_>) -> Result<(), Error> {
        self.0.expand_boxed(expansion)
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
