#![no_std]
#![allow(clippy::cast_possible_truncation)]

#[doc(hidden)]
#[macro_export]
macro_rules! __wire_enum {
    (
        $repr:ident, $from:ident, $as:ident, [$($derive:path),*],
        $(#[$attr:meta])* $vis:vis enum $name:ident {
            $($(#[$variant_attr:meta])* $variant:ident = $value:expr,)+
        }
    ) => {
        $(#[$attr])*
        #[derive(
            Clone,
            Copy,
            PartialEq,
            Eq,
            Debug,
            ::zerocopy::TryFromBytes,
            ::zerocopy::IntoBytes,
            ::zerocopy::KnownLayout,
            ::zerocopy::Immutable,
            $($derive,)*
        )]
        #[repr($repr)]
        #[non_exhaustive]
        $vis enum $name {
            $($(#[$variant_attr])* $variant = $value,)+
        }

        impl $name {
            $vis const ALL: &'static [Self] = &[$(Self::$variant,)+];

            #[must_use]
            $vis const fn $from(value: $repr) -> Option<Self> {
                $(if value == $value {
                    return Some(Self::$variant);
                })+
                None
            }

            #[must_use]
            $vis const fn $as(self) -> $repr {
                self as $repr
            }
        }

        impl ::core::convert::TryFrom<$repr> for $name {
            type Error = $repr;

            fn try_from(value: $repr) -> ::core::result::Result<Self, $repr> {
                Self::$from(value).ok_or(value)
            }
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! wire_enum_u8 {
    ($($body:tt)+) => {
        $crate::__wire_enum! { u8, from_u8, as_u8, [::zerocopy::Unaligned], $($body)+ }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! wire_enum_u32 {
    ($($body:tt)+) => {
        $crate::__wire_enum! { u32, from_u32, as_u32, [], $($body)+ }
    };
}

mod bank;
mod cmd;
pub mod config;
pub mod cpu_params;
mod error;
pub mod fpga_params;
pub mod fpga_update;
mod frame;
pub mod layout;
pub mod payload;
mod telemetry;
pub mod udp;
pub mod update;
pub mod value;

pub use bank::{ModulationBank, PatternBank};
pub use cmd::Cmd;
pub use error::{Error, describe_device_error};
pub use frame::{FRAME_BYTES_MAX, FrameHeader, PAYLOAD_BYTES, REPLY_DATA_BYTES_MAX};
pub use telemetry::Telemetry;
