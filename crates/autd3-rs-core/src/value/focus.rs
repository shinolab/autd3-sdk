use crate::error::Error;
use crate::params::{
    FOCUS_COORD_MAX_X, FOCUS_COORD_MAX_Y, FOCUS_COORD_MAX_Z, FOCUS_COORD_MIN_X, FOCUS_COORD_MIN_Y,
    FOCUS_COORD_MIN_Z,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Focus {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub intensity_or_offset: u8,
}

impl Focus {
    pub fn encode(self) -> Result<u64, Error> {
        for (name, v, min, max) in [
            ("x", self.x, FOCUS_COORD_MIN_X, FOCUS_COORD_MAX_X),
            ("y", self.y, FOCUS_COORD_MIN_Y, FOCUS_COORD_MAX_Y),
            ("z", self.z, FOCUS_COORD_MIN_Z, FOCUS_COORD_MAX_Z),
        ] {
            if !(min..=max).contains(&v) {
                return Err(Error::InvalidPayload(format!(
                    "focus {name} = {v} out of range {min}..={max}"
                )));
            }
        }
        let mask = |v: i32| u64::from(u32::from_le_bytes(v.to_le_bytes())) & 0x3_FFFF;
        Ok(mask(self.x)
            | mask(self.y) << 18
            | mask(self.z) << 36
            | u64::from(self.intensity_or_offset) << 54)
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
        use crate::params::{
            FOCUS_COORD_MIN, FOCUS_COORD_MIN_X, FOCUS_COORD_MIN_Y, FOCUS_COORD_MIN_Z,
        };

        const {
            assert!(FOCUS_COORD_MIN_X > FOCUS_COORD_MIN);
            assert!(FOCUS_COORD_MIN_Y > FOCUS_COORD_MIN);
            assert!(FOCUS_COORD_MIN_Z == FOCUS_COORD_MIN);
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

        assert!(at(FOCUS_COORD_MIN_X, 0, 0).is_ok());
        assert!(at(FOCUS_COORD_MIN_X - 1, 0, 0).is_err());
        assert!(at(0, FOCUS_COORD_MIN_Y, 0).is_ok());
        assert!(at(0, FOCUS_COORD_MIN_Y - 1, 0).is_err());
        assert!(at(0, 0, FOCUS_COORD_MIN_Z).is_ok());
        assert!(at(0, 0, FOCUS_COORD_MIN_Z - 1).is_err());
    }
}
