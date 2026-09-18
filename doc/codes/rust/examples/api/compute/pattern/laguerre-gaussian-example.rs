use autd3_rs::geometry::{Autd3, Geometry, Vector3, offset};
use autd3_rs::units::{m, mm, s};
use autd3_rs_pattern::{
    LaguerreGaussianOption, laguerre_gaussian_phase, laguerre_gaussian_intensity, wavelength,
};

// HIDE
fn main() {
    // HIDE_END
    let geometry = Geometry::new(vec![Autd3::default()]);

    let mut dst = geometry.pattern_buffer();

    let target = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let option = LaguerreGaussianOption {
        p: 0,
        l: 1,
        waist: 10.0 * mm,
    };
    let wavelength = wavelength(340.0 * m / s);
    laguerre_gaussian_phase(
        &geometry,
        target,
        Vector3::z_axis(),
        option,
        wavelength,
        &mut dst,
    );
    laguerre_gaussian_intensity(
        &geometry,
        target,
        Vector3::z_axis(),
        option,
        wavelength,
        &mut dst,
    );
    // HIDE
}
// HIDE_END
