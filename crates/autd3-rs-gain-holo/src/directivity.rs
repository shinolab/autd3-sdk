#[allow(clippy::excessive_precision, clippy::unreadable_literal)]
static DIR_COEF_A: &[f32] = &[
    1.0,
    1.0,
    1.0,
    0.891250938,
    0.707945784,
    0.501187234,
    0.354813389,
    0.251188643,
    0.199526231,
];

#[allow(clippy::excessive_precision, clippy::unreadable_literal)]
static DIR_COEF_B: &[f32] = &[
    0.,
    0.,
    -0.00459648054721,
    -0.0155520765675,
    -0.0208114779827,
    -0.0182211227016,
    -0.0122437497109,
    -0.00780345575475,
    -0.00312857467007,
];

#[allow(clippy::excessive_precision, clippy::unreadable_literal)]
static DIR_COEF_C: &[f32] = &[
    0.,
    0.,
    -0.000787968093807,
    -0.000307591508224,
    -0.000218348633296,
    0.00047738416141,
    0.000120353137658,
    0.000323676257958,
    0.000143850511,
];

#[allow(clippy::excessive_precision, clippy::unreadable_literal)]
static DIR_COEF_D: &[f32] = &[
    0.,
    0.,
    1.60125528528e-05,
    2.9747624976e-06,
    2.31910931569e-05,
    -1.1901034125e-05,
    6.77743734332e-06,
    -5.99548024824e-06,
    -4.79372835035e-06,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Directivity {
    #[default]
    Sphere,
    T4010A1,
}

impl Directivity {
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn value(self, theta_rad: f32) -> f32 {
        match self {
            Directivity::Sphere => 1.0,
            Directivity::T4010A1 => {
                let theta_deg = theta_rad.to_degrees().abs() % 180.0;
                let theta_deg = 90.0 - (theta_deg - 90.0).abs();
                let i = (theta_deg / 10.0).ceil() as usize;
                if i == 0 {
                    1.0
                } else {
                    let idx = i - 1;
                    let x = theta_deg - idx as f32 * 10.0;
                    ((DIR_COEF_D[idx] * x + DIR_COEF_C[idx]) * x + DIR_COEF_B[idx]) * x
                        + DIR_COEF_A[idx]
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sphere_is_unit() {
        for theta in [0.0, 0.3, 1.0, 2.5] {
            approx::assert_abs_diff_eq!(1.0, Directivity::Sphere.value(theta));
        }
    }

    #[test]
    #[allow(clippy::unreadable_literal)]
    fn t4010a1() {
        for (expected, theta_deg) in [
            (1.0_f32, 0.0_f32),
            (1.0, 10.0),
            (1.0, 20.0),
            (0.891251, 30.0),
            (0.70794576, 40.0),
            (0.5011872, 50.0),
            (0.35481337, 60.0),
            (0.25118864, 70.0),
            (0.19952622, 80.0),
            (0.17783181, 90.0),
            (0.19952622, 100.0),
        ] {
            approx::assert_abs_diff_eq!(
                expected,
                Directivity::T4010A1.value(theta_deg.to_radians()),
                epsilon = 1e-5
            );
        }
    }
}
