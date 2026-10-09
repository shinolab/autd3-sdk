use super::Intensity;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatternIntensity<'a> {
    Uniform(Intensity),
    PerDevice(&'a [Vec<Intensity>]),
}

impl Default for PatternIntensity<'_> {
    fn default() -> Self {
        PatternIntensity::Uniform(Intensity::MAX)
    }
}

impl From<Intensity> for PatternIntensity<'_> {
    fn from(value: Intensity) -> Self {
        PatternIntensity::Uniform(value)
    }
}

impl<'a> From<&'a [Vec<Intensity>]> for PatternIntensity<'a> {
    fn from(value: &'a [Vec<Intensity>]) -> Self {
        PatternIntensity::PerDevice(value)
    }
}

impl<'a> From<&'a Vec<Vec<Intensity>>> for PatternIntensity<'a> {
    fn from(value: &'a Vec<Vec<Intensity>>) -> Self {
        PatternIntensity::PerDevice(value.as_slice())
    }
}

impl<'a, const N: usize> From<&'a [Vec<Intensity>; N]> for PatternIntensity<'a> {
    fn from(value: &'a [Vec<Intensity>; N]) -> Self {
        PatternIntensity::PerDevice(value.as_slice())
    }
}
