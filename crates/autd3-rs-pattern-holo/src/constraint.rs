use autd3_rs_core::value::Intensity;

#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum IntensityConstraint {
    Multiply(f32),
    Clamp(Intensity, Intensity),
}

impl IntensityConstraint {
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub(crate) fn convert(self, value: f32, max_value: f32) -> Intensity {
        match self {
            IntensityConstraint::Multiply(v) => {
                Intensity((value / max_value * 255. * v).round().clamp(0., 255.) as u8)
            }
            IntensityConstraint::Clamp(min, max) => Intensity(
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
    use rstest::rstest;

    #[rstest]
    #[case(Intensity::MIN, 0.0, 1.0, 0.5)]
    #[case(Intensity(64), 0.5, 1.0, 0.5)]
    #[case(Intensity(64), 1.0, 2.0, 0.5)]
    #[case(Intensity(96), 1.5, 2.0, 0.5)]
    fn multiply(#[case] expect: Intensity, #[case] value: f32, #[case] max: f32, #[case] mul: f32) {
        assert_eq!(
            expect,
            IntensityConstraint::Multiply(mul).convert(value, max)
        );
    }

    #[rstest]
    #[case(Intensity(64), 0.0, 1.0, Intensity(64), Intensity(192))]
    #[case(Intensity(128), 0.5, 1.0, Intensity(64), Intensity(192))]
    #[case(Intensity(192), 1.0, 1.0, Intensity(64), Intensity(192))]
    #[case(Intensity(192), 1.5, 1.0, Intensity(64), Intensity(192))]
    fn clamp(
        #[case] expect: Intensity,
        #[case] value: f32,
        #[case] max: f32,
        #[case] min: Intensity,
        #[case] mx: Intensity,
    ) {
        assert_eq!(
            expect,
            IntensityConstraint::Clamp(min, mx).convert(value, max)
        );
    }
}
