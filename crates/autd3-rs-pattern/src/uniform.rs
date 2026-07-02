use autd3_rs_core::value::Emission;

pub fn uniform_device(emission: Emission, out: &mut [Emission]) {
    for slot in out.iter_mut() {
        *slot = emission;
    }
}

pub fn uniform(emission: Emission, out: &mut [Vec<Emission>]) {
    for slot in &mut *out {
        uniform_device(emission, slot);
    }
}

#[cfg(test)]
mod tests {
    use autd3_rs_core::geometry::Autd3;
    use autd3_rs_core::value::{Intensity, Phase};

    use super::*;

    #[test]
    fn uniform_fills_every_transducer() {
        let emission = Emission {
            phase: Phase(0x40),
            intensity: Intensity(0x80),
        };

        let mut out = vec![vec![Emission::default(); Autd3::NUM_TRANSDUCERS]; 2];
        uniform(emission, &mut out);

        for dev in &out {
            for &e in dev {
                assert_eq!(e, emission);
            }
        }
    }
}
