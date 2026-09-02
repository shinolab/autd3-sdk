use autd3_rs::geometry::{Autd3, Geometry, Vector3, offset};
use autd3_rs::units::{m, mm, s};
use autd3_rs::value::{Emission, Intensity, Phase};
use autd3_rs_pattern::{VortexOption, vortex, wavelength};

fn main() {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let target = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let axis = Vector3::z_axis();
    let order = 1;
    let wavelength = wavelength(340.0 * m / s);
    let intensity = Intensity::MAX;
    let phase_offset = Phase::ZERO;
    let option =
        // ANCHOR: option
        VortexOption {
            intensity,
            phase_offset,
            ..Default::default()
        }
        // ANCHOR_END: option
        ;
    let mut dst = vec![vec![Emission::default(); Autd3::NUM_TRANSDUCERS]; geometry.num_devices()];

    // ANCHOR: api
    vortex(
        &geometry, target, axis, order, wavelength, &option, &mut dst,
    );
    // ANCHOR_END: api
}
