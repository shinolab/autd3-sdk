use autd3_rs::geometry::{Autd3, Geometry, Vector3};
use autd3_rs::units::{m, s};
use autd3_rs::value::{Emission, Intensity, Phase};
use autd3_rs_pattern::{PlaneOption, plane, wavelength};

fn main() {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let direction = Vector3::z_axis();
    let wavelength = wavelength(340.0 * m / s);
    let intensity = Intensity::MAX;
    let phase_offset = Phase::ZERO;
    let option =
        // ANCHOR: option
        PlaneOption {
            intensity,
            phase_offset,
            ..Default::default()
        }
        // ANCHOR_END: option
        ;
    let mut dst = vec![vec![Emission::default(); Autd3::NUM_TRANSDUCERS]; geometry.num_devices()];

    // ANCHOR: api
    plane(&geometry, direction, wavelength, &option, &mut dst);
    // ANCHOR_END: api
}
