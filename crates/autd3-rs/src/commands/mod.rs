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
    ConfigPattern, Distribution, EmulateGpioIn, Encoded, FixedCompletionTime, FixedUpdateRate,
    ForceFan, GpioOut, Nop, Operation, PWE_TABLE_SIZE, PatternIntensity, PhaseDepth, SetGpioOut,
    SetOutputMask, SetPhaseCorrection, SetPulseWidthTable, SetSilencer, SilencerConfig,
    Synchronize, WritePatternBuffer, WritePatternPhase,
};
pub use stm::{
    FociStm, FociStmOption, PatternStm, PatternStmOption, StmConfig, StmIntensity, circle, line,
};

use crate::datagram::DatagramBuilder;

pub trait Command<'a> {
    fn expand(self, builder: &mut DatagramBuilder<'a>);

    #[must_use]
    fn boxed(self) -> BoxedCommand<'a>
    where
        Self: Sized + 'a,
    {
        BoxedCommand(Box::new(self))
    }
}

impl<'a, O: Operation + 'a> Command<'a> for O {
    fn expand(self, builder: &mut DatagramBuilder<'a>) {
        builder.push_op(self);
    }
}

trait DynCommand<'a> {
    fn expand_boxed(self: Box<Self>, builder: &mut DatagramBuilder<'a>);
}

impl<'a, C: Command<'a>> DynCommand<'a> for C {
    fn expand_boxed(self: Box<Self>, builder: &mut DatagramBuilder<'a>) {
        (*self).expand(builder);
    }
}

pub struct BoxedCommand<'a>(Box<dyn DynCommand<'a> + 'a>);

impl<'a> Command<'a> for BoxedCommand<'a> {
    fn expand(self, builder: &mut DatagramBuilder<'a>) {
        self.0.expand_boxed(builder);
    }
}
