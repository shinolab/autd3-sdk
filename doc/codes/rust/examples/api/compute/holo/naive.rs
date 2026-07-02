use anyhow::Result;

use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::units::{m, mm, s};
use autd3_rs::value::{Emission, Intensity};
use autd3_rs_pattern_holo::{
    ControlPoint, Directivity, EmissionConstraint, NaiveOption, NalgebraBackend, Pa,
    TransducerMask, naive,
};

fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let foci = [
        ControlPoint {
            point: center + offset(-30.0 * mm, 0.0 * mm, 0.0 * mm),
            amplitude: 2.5e3 * Pa,
        },
        ControlPoint {
            point: center + offset(30.0 * mm, 0.0 * mm, 0.0 * mm),
            amplitude: 2.5e3 * Pa,
        },
    ];

    let wavelength = autd3_rs_pattern::wavelength(340.0 * m / s);
    let option =
        // ANCHOR: option
        NaiveOption {
            constraint: EmissionConstraint::Clamp(Intensity::MIN, Intensity::MAX),
            directivity: Directivity::Sphere,
            backend: NalgebraBackend,
            mask: TransducerMask::AllEnabled,
        }
        // ANCHOR_END: option
        ;
    let mut out = vec![vec![Emission::default(); Autd3::NUM_TRANSDUCERS]; geometry.num_devices()];
    // ANCHOR: api
    naive(&geometry, &foci, wavelength, &option, &mut out)?;
    // ANCHOR_END: api
    Ok(())
}
