use crate::fpga_params::{FOCUS_TR_X_MAX, FOCUS_TR_Y_MAX};
use crate::layout::{FOCUS_COORD_MAX, FOCUS_COORD_MIN};

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("focus coordinate {axis} = {value} out of range {min}..={max}")]
pub struct FocusOutOfRange {
    pub axis: &'static str,
    pub value: i32,
    pub min: i32,
    pub max: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
#[doc(hidden)]
pub enum FocusKind {
    Point = 0,
    Bessel = 1,
    Plane = 2,
    Vortex = 3,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub struct Focus {
    pub kind: FocusKind,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub intensity_or_offset: u8,
}

impl Focus {
    pub fn encode(self) -> Result<u64, FocusOutOfRange> {
        let max = FOCUS_COORD_MAX;
        let (x_min, y_min) = match self.kind {
            FocusKind::Plane => (FOCUS_COORD_MIN, FOCUS_COORD_MIN),
            FocusKind::Point | FocusKind::Bessel | FocusKind::Vortex => (
                FOCUS_COORD_MIN + FOCUS_TR_X_MAX,
                FOCUS_COORD_MIN + FOCUS_TR_Y_MAX,
            ),
        };
        for (name, v, min) in [
            ("x", self.x, x_min),
            ("y", self.y, y_min),
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
        Ok(u64::from(self.kind as u8) << 62
            | u64::from(self.intensity_or_offset) << 54
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
            kind: FocusKind::Point,
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
        assert_eq!(v >> 62, 0, "point focus keeps the legacy zero kind bits");
    }

    #[test]
    fn focus_kind_goes_to_the_top_two_bits() {
        for (kind, bits) in [
            (FocusKind::Point, 0b00),
            (FocusKind::Bessel, 0b01),
            (FocusKind::Plane, 0b10),
            (FocusKind::Vortex, 0b11),
        ] {
            let v = Focus {
                kind,
                x: 0x1_2345,
                y: -0x1_0000,
                z: 0x0_ABCD,
                intensity_or_offset: 0x5A,
            }
            .encode()
            .unwrap();
            assert_eq!(
                v,
                (bits << 62) | 0x5A_u64 << 54 | 0x0_ABCD_u64 << 36 | 0x3_0000_u64 << 18 | 0x1_2345
            );
        }
    }

    #[test]
    fn plane_coefficients_use_the_full_18_bit_range() {
        let at = |kind, x, y| {
            Focus {
                kind,
                x,
                y,
                z: 0,
                intensity_or_offset: 0,
            }
            .encode()
        };
        assert!(at(FocusKind::Plane, FOCUS_COORD_MIN, FOCUS_COORD_MIN).is_ok());
        assert!(at(FocusKind::Plane, FOCUS_COORD_MIN - 1, 0).is_err());
        assert!(at(FocusKind::Plane, 0, FOCUS_COORD_MAX + 1).is_err());
        for kind in [FocusKind::Bessel, FocusKind::Vortex] {
            assert!(at(kind, FOCUS_COORD_MIN + FOCUS_TR_X_MAX - 1, 0).is_err());
            assert!(at(kind, 0, FOCUS_COORD_MIN + FOCUS_TR_Y_MAX - 1).is_err());
        }
    }

    #[test]
    fn focus_x_y_lower_bound_is_narrowed() {
        const {
            assert!(FOCUS_TR_X_MAX > 0);
            assert!(FOCUS_TR_Y_MAX > 0);
        }

        let at = |x, y, z| {
            Focus {
                kind: FocusKind::Point,
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
