mod modulation;
mod pattern;

pub use modulation::Modulation;
pub use pattern::Pattern;

use crate::datagram::DatagramBuilder;
use crate::operation::Operation;

pub trait Command<'a> {
    fn expand(self, builder: &mut DatagramBuilder<'a>);
}

impl<'a, O: Operation + 'a> Command<'a> for O {
    fn expand(self, builder: &mut DatagramBuilder<'a>) {
        builder.push_op(self);
    }
}
