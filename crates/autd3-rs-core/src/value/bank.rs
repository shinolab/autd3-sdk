macro_rules! bank_enum {
    ($name:ident) => {
        #[repr(u8)]
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
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
