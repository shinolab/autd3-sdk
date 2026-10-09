mod modulation;
pub(crate) mod operation;
mod pattern;
pub(crate) mod stm;
mod write_foci_buffer;
mod write_modulation_buffer;

pub use modulation::Modulation;
pub use pattern::Pattern;
pub use write_foci_buffer::WriteFociBuffer;
pub use write_modulation_buffer::WriteModulationBuffer;

pub use operation::{
    ActivateModulationBank, ActivatePatternBank, Clear, ConfigFociStm, ConfigModulation,
    ConfigPattern, CpuConfig, Distribution, EmulateGpioIn, Encoded, FixedCompletionTime,
    FixedUpdateRate, ForceFan, FpgaBusWait, GpioOut, Nop, Operation, PWE_TABLE_SIZE,
    PatternIntensity, PhaseDepth, PtpConfig, ReleaseFailsafe, SetCpuConfig, SetGpioOut,
    SetOutputMask, SetPhaseCorrection, SetPulseWidthTable, SetSilencer, SilencerConfig,
    StmIntensity, Synchronize, WritePatternBuffer, WritePatternPhase,
};
pub use stm::{FociStm, FociStmOption, PatternStm, PatternStmOption, StmConfig, circle, line};

pub use crate::datagram::{Each, Expansion, each};
use crate::error::Error;

pub trait Command<'a> {
    fn expand(self, expansion: &mut Expansion<'_, 'a>) -> Result<(), Error>;

    #[must_use]
    fn boxed(self) -> BoxedCommand<'a>
    where
        Self: Sized + 'a,
    {
        BoxedCommand(Box::new(self))
    }
}

impl<'a, O: Operation + 'a> Command<'a> for O {
    fn expand(self, expansion: &mut Expansion<'_, 'a>) -> Result<(), Error> {
        expansion.push_op(self);
        Ok(())
    }
}

trait DynCommand<'a> {
    fn expand_boxed(self: Box<Self>, expansion: &mut Expansion<'_, 'a>) -> Result<(), Error>;
}

impl<'a, C: Command<'a>> DynCommand<'a> for C {
    fn expand_boxed(self: Box<Self>, expansion: &mut Expansion<'_, 'a>) -> Result<(), Error> {
        (*self).expand(expansion)
    }
}

pub struct BoxedCommand<'a>(Box<dyn DynCommand<'a> + 'a>);

impl<'a> Command<'a> for BoxedCommand<'a> {
    fn expand(self, expansion: &mut Expansion<'_, 'a>) -> Result<(), Error> {
        self.0.expand_boxed(expansion)
    }
}

macro_rules! impl_command_for_tuple {
    ($($name:ident),+) => {
        impl<'a, $($name: Command<'a>),+> Command<'a> for ($($name,)+) {
            #[allow(non_snake_case)]
            fn expand(self, expansion: &mut Expansion<'_, 'a>) -> Result<(), Error> {
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
