use autd3_rs::geometry::{Autd3, Geometry, Vector3, offset};
use autd3_rs::units::{m, mm, s};
use autd3_rs_pattern::{
    HermiteGaussianOption, hermite_gaussian_phase, hermite_gaussian_intensity, wavelength,
};

// HIDE
fn main() {
    // HIDE_END
    let geometry = Geometry::new(vec![Autd3::default()]);

    let mut phases = geometry.phase_buffer();
    let mut intensities = geometry.intensity_buffer();

    let target = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let option = HermiteGaussianOption {
        m: 1,
        n: 1,
        waist: 10.0 * mm,
    };
    let wavelength = wavelength(340.0 * m / s);
    hermite_gaussian_phase(
        &geometry,
        target,
        Vector3::z_axis(),
        Vector3::x_axis(),
        option,
        wavelength,
        &mut phases,
    );
    hermite_gaussian_intensity(
        &geometry,
        target,
        Vector3::z_axis(),
        Vector3::x_axis(),
        option,
        wavelength,
        &mut intensities,
    );
    // HIDE
}
// HIDE_END
