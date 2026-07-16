#![no_std]

#[macro_export]
macro_rules! wire_enum {
    ($vis:vis enum $name:ident { $($variant:ident = $value:expr,)+ }) => {
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        #[repr(u8)]
        $vis enum $name {
            $($variant = $value,)+
        }

        impl $name {
            #[must_use]
            $vis const fn from_u8(value: u8) -> Option<Self> {
                $(if value == $value {
                    return Some(Self::$variant);
                })+
                None
            }

            #[must_use]
            $vis const fn as_u8(self) -> u8 {
                self as u8
            }
        }

        impl ::core::convert::TryFrom<u8> for $name {
            type Error = u8;

            fn try_from(value: u8) -> ::core::result::Result<Self, u8> {
                Self::from_u8(value).ok_or(value)
            }
        }
    };
}

mod cmd;
mod error;
mod frame;
pub mod layout;
mod mode;
pub mod params;
pub mod payload;
mod telemetry;

pub use cmd::Cmd;
pub use error::{Error, describe_device_error};
pub use frame::{DEVICE_TO_HOST_BYTES, HOST_TO_DEVICE_BYTES, PAYLOAD_BYTES};
pub use mode::Mode;
pub use telemetry::Telemetry;
