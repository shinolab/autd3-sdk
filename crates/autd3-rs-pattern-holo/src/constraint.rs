use autd3_rs_core::value::Intensity;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EmissionConstraint {
    Normalize,
    Multiply(f32),
    Uniform(Intensity),
    Clamp(Intensity, Intensity),
}

impl EmissionConstraint {
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn convert(self, value: f32, max_value: f32) -> Intensity {
        match self {
            EmissionConstraint::Normalize => Intensity((value / max_value * 255.).round() as u8),
            EmissionConstraint::Multiply(v) => {
                Intensity((value / max_value * 255. * v).round().clamp(0., 255.) as u8)
            }
            EmissionConstraint::Uniform(v) => v,
            EmissionConstraint::Clamp(min, max) => Intensity(
                (value * 255.)
                    .round()
                    .clamp(f32::from(min.0), f32::from(max.0)) as u8,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize() {
        for (expect, value, max) in [
            (Intensity::MIN, 0.0, 1.0),
            (Intensity(128), 0.5, 1.0),
            (Intensity(128), 1.0, 2.0),
            (Intensity(191), 1.5, 2.0),
        ] {
            assert_eq!(expect, EmissionConstraint::Normalize.convert(value, max));
        }
    }

    #[test]
    fn multiply() {
        for (expect, value, max, mul) in [
            (Intensity::MIN, 0.0, 1.0, 0.5),
            (Intensity(64), 0.5, 1.0, 0.5),
            (Intensity(64), 1.0, 2.0, 0.5),
            (Intensity(96), 1.5, 2.0, 0.5),
        ] {
            assert_eq!(
                expect,
                EmissionConstraint::Multiply(mul).convert(value, max)
            );
        }
    }

    #[test]
    fn uniform() {
        for (expect, value, max) in [
            (Intensity::MIN, 0.0, 1.0),
            (Intensity::MAX, 0.5, 1.0),
            (Intensity(128), 1.5, 2.0),
        ] {
            assert_eq!(
                expect,
                EmissionConstraint::Uniform(expect).convert(value, max)
            );
        }
    }

    #[test]
    fn clamp() {
        for (expect, value, max, min, mx) in [
            (Intensity(64), 0.0, 1.0, Intensity(64), Intensity(192)),
            (Intensity(128), 0.5, 1.0, Intensity(64), Intensity(192)),
            (Intensity(192), 1.0, 1.0, Intensity(64), Intensity(192)),
            (Intensity(192), 1.5, 1.0, Intensity(64), Intensity(192)),
        ] {
            assert_eq!(
                expect,
                EmissionConstraint::Clamp(min, mx).convert(value, max)
            );
        }
    }
}
