use autd3_rs::geometry::{Autd3, Geometry, Vector3, offset};
use autd3_rs::units::{m, mm, s};
use autd3_rs_pattern::{
    HermiteGaussianOption, hermite_gaussian_phase, hermite_gaussian_intensity, wavelength,
};

fn main() {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let target = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let axis = Vector3::z_axis();
    let x_dir = Vector3::x_axis();
    let wavelength = wavelength(340.0 * m / s);
    let mut phases = geometry.phase_buffer();
    let mut intensities = geometry.intensity_buffer();

    // ANCHOR: api
    let option = HermiteGaussianOption {
        m: 1,
        n: 0,
        waist: 10.0 * mm,
    };
    hermite_gaussian_phase(&geometry, target, axis, x_dir, option, wavelength, &mut phases);
    hermite_gaussian_intensity(&geometry, target, axis, x_dir, option, wavelength, &mut intensities);
    // ANCHOR_END: api
}
