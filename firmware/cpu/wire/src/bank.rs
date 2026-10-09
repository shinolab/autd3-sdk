use zerocopy::{Immutable, IntoBytes, KnownLayout, TryFromBytes, Unaligned};

use crate::fpga_params::NUM_BANKS;

macro_rules! bank_enum {
    ($name:ident) => {
        #[derive(
            Clone,
            Copy,
            Debug,
            Default,
            PartialEq,
            Eq,
            TryFromBytes,
            IntoBytes,
            KnownLayout,
            Immutable,
            Unaligned,
        )]
        #[repr(u8)]
        pub enum $name {
            #[default]
            B0 = 0,
            B1 = 1,
        }

        impl $name {
            #[must_use]
            pub const fn as_u8(self) -> u8 {
                self as u8
            }
        }
    };
}

bank_enum!(PatternBank);
bank_enum!(ModulationBank);

const _: () = assert!(NUM_BANKS == 2);
