use anyhow::Result;

use autd3_rs::geometry::{Autd3, Geometry, TransducerGroups, offset};
use autd3_rs::units::{m, mm, s};
use autd3_rs_pattern::{focus, group_compute, wavelength};
use autd3_rs_pattern_holo::{AmplitudeTarget, GspatOption, NalgebraBackend, Pa, gspat};

// HIDE
fn main() -> Result<()> {
    // HIDE_END
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Side {
        Left,
        Right,
    }

    let geometry = Geometry::new(vec![Autd3::default()]);
    let wavelength = wavelength(340.0 * m / s);
    let center = geometry.center();

    let groups = TransducerGroups::new(&geometry, |device, tr| {
        Some(if device.position(tr).x < center.x {
            Side::Left
        } else {
            Side::Right
        })
    });

    let foci = [
        AmplitudeTarget {
            point: center + offset(-50.0 * mm, 0.0 * mm, 150.0 * mm),
            amplitude: 5e3 * Pa,
        },
        AmplitudeTarget {
            point: center + offset(-20.0 * mm, 0.0 * mm, 150.0 * mm),
            amplitude: 5e3 * Pa,
        },
    ];

    let mut dst = geometry.pattern_buffer();
    group_compute(
        &geometry,
        &groups,
        |side, mask, buffer| match side {
            Side::Left => gspat(
                &NalgebraBackend,
                &geometry,
                &foci,
                wavelength,
                &GspatOption {
                    mask,
                    ..Default::default()
                },
                buffer,
            ),
            Side::Right => {
                focus(
                    &geometry,
                    center + offset(40.0 * mm, 0.0 * mm, 150.0 * mm),
                    wavelength,
                    buffer,
                );
                Ok(())
            }
        },
        &mut dst,
    )?;
    // HIDE
    Ok(())
}
// HIDE_END
