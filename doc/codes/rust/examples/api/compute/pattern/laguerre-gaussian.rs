use autd3_rs::geometry::{Autd3, Geometry, Vector3, offset};
use autd3_rs::units::{m, mm, s};
use autd3_rs_pattern::{
    LaguerreGaussianOption, laguerre_gaussian_phase, laguerre_gaussian_intensity, wavelength,
};

fn main() {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let target = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let axis = Vector3::z_axis();
    let wavelength = wavelength(340.0 * m / s);
    let mut dst = geometry.pattern_buffer();

    // ANCHOR: api
    let option = LaguerreGaussianOption {
        p: 1,
        l: 1,
        waist: 10.0 * mm,
    };
    laguerre_gaussian_phase(&geometry, target, axis, option, wavelength, &mut dst);
    laguerre_gaussian_intensity(&geometry, target, axis, option, wavelength, &mut dst);
    // ANCHOR_END: api
}
