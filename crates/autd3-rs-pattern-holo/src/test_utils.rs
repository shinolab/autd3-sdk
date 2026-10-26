use autd3_rs_core::common::Length;
use autd3_rs_core::common::units::{m, s};
use autd3_rs_core::geometry::{Autd3, Geometry, Point3, TransducerMask, UnitQuaternion};
use autd3_rs_core::value::{Intensity, Phase};

use crate::amplitude_target::AmplitudeTarget;
use crate::backend::NalgebraBackend;
use crate::error::HoloError;
use crate::linear_synthesis::{GsOption, GspatOption, NaiveOption, gs, gspat, naive};

pub(crate) type Slot = (Vec<Vec<Phase>>, Vec<Vec<Intensity>>);

pub(crate) type Single =
    fn(&Geometry, &[AmplitudeTarget], TransducerMask<'_>, &mut Slot) -> Result<(), HoloError>;

pub(crate) fn geometry(devices: usize) -> Geometry {
    Geometry::new((0..devices).map(|i| {
        Autd3::new(
            Point3::new(i as f32 * 200.0, 0.0, 0.0),
            UnitQuaternion::identity(),
        )
    }))
}

pub(crate) fn wavelength() -> Length {
    autd3_rs_pattern::wavelength(340.0 * m / s)
}

pub(crate) fn slot(geometry: &Geometry) -> Slot {
    (geometry.phase_buffer(), geometry.intensity_buffer())
}

macro_rules! algorithm {
    ($name:literal, $single:ident, $option:ident) => {
        ($name, |geometry, foci, mask, dst| {
            $single(
                &NalgebraBackend,
                geometry,
                foci,
                wavelength(),
                &$option {
                    mask,
                    ..Default::default()
                },
                &mut dst.0,
                &mut dst.1,
            )
        })
    };
}

pub(crate) const ALGORITHMS: [(&str, Single); 3] = [
    algorithm!("naive", naive, NaiveOption),
    algorithm!("gs", gs, GsOption),
    algorithm!("gspat", gspat, GspatOption),
];
