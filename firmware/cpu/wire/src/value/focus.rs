use crate::fpga_params::{FOCUS_TR_X_MAX, FOCUS_TR_Y_MAX};
use crate::layout::{FOCUS_COORD_MAX, FOCUS_COORD_MIN};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FocusOutOfRange {
    pub axis: &'static str,
    pub value: i32,
    pub min: i32,
    pub max: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[doc(hidden)]
pub struct Focus {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub intensity_or_offset: u8,
}

impl Focus {
    pub fn encode(self) -> Result<u64, FocusOutOfRange> {
        let max = FOCUS_COORD_MAX;
        for (name, v, min) in [
            ("x", self.x, FOCUS_COORD_MIN + FOCUS_TR_X_MAX),
            ("y", self.y, FOCUS_COORD_MIN + FOCUS_TR_Y_MAX),
            ("z", self.z, FOCUS_COORD_MIN),
        ] {
            if !(min..=max).contains(&v) {
                return Err(FocusOutOfRange {
                    axis: name,
                    value: v,
                    min,
                    max,
                });
            }
        }
        let mask = |v: i32| u64::from(u32::from_le_bytes(v.to_le_bytes())) & 0x3_FFFF;
        Ok(u64::from(self.intensity_or_offset) << 54
            | mask(self.z) << 36
            | mask(self.y) << 18
            | mask(self.x))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_encodes_legacy_bit_layout() {
        let f = Focus {
            x: -1,
            y: 2,
            z: -131_072,
            intensity_or_offset: 0xAB,
        };
        let v = f.encode().unwrap();
        assert_eq!(v & 0x3_FFFF, 0x3_FFFF, "x = -1 sign-extends to all ones");
        assert_eq!((v >> 18) & 0x3_FFFF, 2);
        assert_eq!((v >> 36) & 0x3_FFFF, 0x2_0000, "z = i18::MIN");
        assert_eq!((v >> 54) & 0xFF, 0xAB);
    }

    #[test]
    fn focus_x_y_lower_bound_is_narrowed() {
        const {
            assert!(FOCUS_TR_X_MAX > 0);
            assert!(FOCUS_TR_Y_MAX > 0);
        }

        let at = |x, y, z| {
            Focus {
                x,
                y,
                z,
                intensity_or_offset: 0,
            }
            .encode()
        };

        assert!(at(FOCUS_COORD_MIN + FOCUS_TR_X_MAX, 0, 0).is_ok());
        assert!(at(FOCUS_COORD_MIN + FOCUS_TR_X_MAX - 1, 0, 0).is_err());
        assert!(at(0, FOCUS_COORD_MIN + FOCUS_TR_Y_MAX, 0).is_ok());
        assert!(at(0, FOCUS_COORD_MIN + FOCUS_TR_Y_MAX - 1, 0).is_err());
        assert!(at(0, 0, FOCUS_COORD_MIN).is_ok());
        assert!(at(0, 0, FOCUS_COORD_MIN - 1).is_err());
    }
}
