mod modulation;
pub(crate) mod operation;
mod pattern;
pub(crate) mod stm;

pub use modulation::Modulation;
pub use pattern::Pattern;

pub use operation::{
    ChangeModulationBank, ChangePatternBank, Clear, ConfigFociStm, ConfigModulation, ConfigPattern,
    Distribution, EmulateGpioIn, FixedCompletionTime, FixedUpdateRate, ForceFan, GpioOut, Nop,
    Operation, PWE_TABLE_SIZE, PatternCompression, SetGpioOut, SetOutputMask, SetPhaseCorrection,
    SetPulseWidthTable, SetSilencer, SilencerConfig, Synchronize, WriteFociBuffer,
    WriteModulationBuffer, WritePatternBuffer, WritePatternCompressed, XOR_HASH_MAX_DATA_LEN,
    XorHashCmd,
};
pub use stm::{
    FociStm, FociStmOption, PatternStm, PatternStmMode, PatternStmOption, StmConfig, circle, line,
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
