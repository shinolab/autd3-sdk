use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::units::{m, mm, s};
use autd3_rs::value::{Emission, Intensity, Phase};
use autd3_rs_pattern::{FocusOption, focus, wavelength};

fn main() {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let target = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let wavelength = wavelength(340.0 * m / s);
    let option =
        // ANCHOR: option
        FocusOption {
            intensity: Intensity::MAX,
            phase_offset: Phase::ZERO,
        }
        // ANCHOR_END: option
        ;
    let mut out = vec![vec![Emission::default(); Autd3::NUM_TRANSDUCERS]; geometry.num_devices()];

    // ANCHOR: api
    focus(&geometry, target, wavelength, &option, &mut out);
    // ANCHOR_END: api
}
