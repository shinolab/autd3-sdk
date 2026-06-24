mod modulation;
mod pattern;

pub use modulation::Modulation;
pub use pattern::Pattern;

use crate::datagram::DatagramBuilder;
use crate::operation::Operation;

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
