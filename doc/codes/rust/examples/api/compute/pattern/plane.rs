use autd3_rs::geometry::{Autd3, Geometry, UnitVector3, Vector3};
use autd3_rs::units::{m, s};
use autd3_rs::value::{Emission, Intensity, Phase};
use autd3_rs_pattern::{PlaneOption, plane, wavelength};

fn main() {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let direction = UnitVector3::new_normalize(Vector3::new(0.0, 0.0, 1.0));
    let wavelength = wavelength(340.0 * m / s);
    let option =
        // ANCHOR: option
        PlaneOption {
            intensity: Intensity::MAX,
            phase_offset: Phase::ZERO,
        }
        // ANCHOR_END: option
        ;
    let mut out = vec![vec![Emission::default(); Autd3::NUM_TRANSDUCERS]; geometry.num_devices()];

    // ANCHOR: api
    plane(&geometry, direction, wavelength, &option, &mut out);
    // ANCHOR_END: api
}
