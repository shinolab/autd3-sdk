#![cfg(test)]

use autd3_rs_core::common::units::{m, s};
use autd3_rs_core::geometry::{Autd3, Geometry, Point3, UnitQuaternion, Vector3};
use autd3_rs_core::value::{Intensity, Phase};

use crate::amp::Pa;
use crate::amplitude_target::AmplitudeTarget;
use crate::backend::NalgebraBackend;
use crate::error::HoloError;
use crate::linear_synthesis::{
    GsOption, GspatOption, NaiveOption, gs, gs_batch, gspat, gspat_batch, naive, naive_batch,
};
use crate::mask::TransducerMask;

fn geometry(devices: usize) -> Geometry {
    Geometry::new(
        (0..devices)
            .map(|i| {
                Autd3::new(
                    Point3::new(i as f32 * 200.0, 0.0, 0.0),
                    UnitQuaternion::identity(),
                )
            })
            .collect(),
    )
}

fn problem(g: &Geometry, seed: usize, nf: usize) -> Vec<AmplitudeTarget> {
    (0..nf)
        .map(|i| AmplitudeTarget {
            point: g.center()
                + Vector3::new(
                    (seed + i) as f32 * 7.0,
                    seed as f32 * -3.0,
                    120.0 + i as f32 * 5.0,
                ),
            amplitude: (3e3 + (seed * nf + i) as f32 * 100.0) * Pa,
        })
        .collect()
}

type Slot = (Vec<Vec<Phase>>, Vec<Vec<Intensity>>);
type Batch = (Vec<Vec<Vec<Phase>>>, Vec<Vec<Vec<Intensity>>>);

fn slot(g: &Geometry) -> Slot {
    (g.phase_buffer(), g.intensity_buffer())
}

fn filled(g: &Geometry, phase: Phase, intensity: Intensity) -> Slot {
    (
        vec![vec![phase; Autd3::NUM_TRANSDUCERS]; g.num_devices()],
        vec![vec![intensity; Autd3::NUM_TRANSDUCERS]; g.num_devices()],
    )
}

fn batch(one: &Slot, n: usize) -> Batch {
    (vec![one.0.clone(); n], vec![one.1.clone(); n])
}

fn problems(b: &Batch) -> impl Iterator<Item = Slot> + '_ {
    b.0.iter().cloned().zip(b.1.iter().cloned())
}

fn wl() -> autd3_rs_core::common::Length {
    autd3_rs_pattern::wavelength(340.0 * m / s)
}

#[test]
fn batch_matches_sequential() {
    let g = geometry(2);
    for nf in [1usize, 4] {
        let owned: Vec<Vec<AmplitudeTarget>> = (0..5).map(|k| problem(&g, k, nf)).collect();
        let foci: Vec<AmplitudeTarget> = owned.concat();

        let mut batched = batch(&slot(&g), owned.len());
        let mut one = slot(&g);

        naive_batch(
            &NalgebraBackend,
            &g,
            &foci,
            wl(),
            &NaiveOption::default(),
            &mut batched.0,
            &mut batched.1,
        )
        .unwrap();
        for (f, want) in owned.iter().zip(problems(&batched)) {
            naive(
                &NalgebraBackend,
                &g,
                f,
                wl(),
                &NaiveOption::default(),
                &mut one.0,
                &mut one.1,
            )
            .unwrap();
            assert_eq!(one, want, "naive {nf} foci");
        }

        gs_batch(
            &NalgebraBackend,
            &g,
            &foci,
            wl(),
            &GsOption::default(),
            &mut batched.0,
            &mut batched.1,
        )
        .unwrap();
        for (f, want) in owned.iter().zip(problems(&batched)) {
            gs(
                &NalgebraBackend,
                &g,
                f,
                wl(),
                &GsOption::default(),
                &mut one.0,
                &mut one.1,
            )
            .unwrap();
            assert_eq!(one, want, "gs {nf} foci");
        }

        gspat_batch(
            &NalgebraBackend,
            &g,
            &foci,
            wl(),
            &GspatOption::default(),
            &mut batched.0,
            &mut batched.1,
        )
        .unwrap();
        for (f, want) in owned.iter().zip(problems(&batched)) {
            gspat(
                &NalgebraBackend,
                &g,
                f,
                wl(),
                &GspatOption::default(),
                &mut one.0,
                &mut one.1,
            )
            .unwrap();
            assert_eq!(one, want, "gspat {nf} foci");
        }
    }
}

#[test]
fn parallel_flag_does_not_change_the_result() {
    let g = geometry(2);
    let masked: Vec<Vec<bool>> = (0..g.num_devices())
        .map(|d| {
            (0..Autd3::NUM_TRANSDUCERS)
                .map(|t| (d + t) % 3 != 0)
                .collect()
        })
        .collect();
    let owned: Vec<Vec<AmplitudeTarget>> = (0..3).map(|k| problem(&g, k, 4)).collect();
    let foci: Vec<AmplitudeTarget> = owned.concat();

    for mask in [TransducerMask::AllEnabled, TransducerMask::Masked(&masked)] {
        let mut on = slot(&g);
        let mut off = slot(&g);
        gs(
            &NalgebraBackend,
            &g,
            &owned[0],
            wl(),
            &GsOption {
                mask,
                parallel: true,
                ..Default::default()
            },
            &mut on.0,
            &mut on.1,
        )
        .unwrap();
        gs(
            &NalgebraBackend,
            &g,
            &owned[0],
            wl(),
            &GsOption {
                mask,
                parallel: false,
                ..Default::default()
            },
            &mut off.0,
            &mut off.1,
        )
        .unwrap();
        assert_eq!(on, off, "single problem");

        let mut on = batch(&slot(&g), owned.len());
        let mut off = batch(&slot(&g), owned.len());
        gs_batch(
            &NalgebraBackend,
            &g,
            &foci,
            wl(),
            &GsOption {
                mask,
                parallel: true,
                ..Default::default()
            },
            &mut on.0,
            &mut on.1,
        )
        .unwrap();
        gs_batch(
            &NalgebraBackend,
            &g,
            &foci,
            wl(),
            &GsOption {
                mask,
                parallel: false,
                ..Default::default()
            },
            &mut off.0,
            &mut off.1,
        )
        .unwrap();
        assert_eq!(on, off, "batch");
    }
}

#[test]
fn all_masked_batch_matches_sequential() {
    let g = geometry(2);
    let masked: Vec<Vec<bool>> = vec![vec![false; Autd3::NUM_TRANSDUCERS]; g.num_devices()];
    let mask = TransducerMask::Masked(&masked);
    let owned: Vec<Vec<AmplitudeTarget>> = (0..3).map(|k| problem(&g, k, 2)).collect();
    let foci: Vec<AmplitudeTarget> = owned.concat();

    let dirty = filled(&g, Phase(0x7F), Intensity(0xFF));
    let inactive = filled(&g, Phase::ZERO, Intensity::MIN);

    let mut one = dirty.clone();
    let mut batched = batch(&dirty, owned.len());
    naive(
        &NalgebraBackend,
        &g,
        &owned[0],
        wl(),
        &NaiveOption {
            mask,
            ..Default::default()
        },
        &mut one.0,
        &mut one.1,
    )
    .unwrap();
    naive_batch(
        &NalgebraBackend,
        &g,
        &foci,
        wl(),
        &NaiveOption {
            mask,
            ..Default::default()
        },
        &mut batched.0,
        &mut batched.1,
    )
    .unwrap();
    assert_eq!(one, inactive, "naive single");
    assert!(problems(&batched).all(|b| b == one), "naive batch");

    let mut one = dirty.clone();
    let mut batched = batch(&dirty, owned.len());
    gs(
        &NalgebraBackend,
        &g,
        &owned[0],
        wl(),
        &GsOption {
            mask,
            ..Default::default()
        },
        &mut one.0,
        &mut one.1,
    )
    .unwrap();
    gs_batch(
        &NalgebraBackend,
        &g,
        &foci,
        wl(),
        &GsOption {
            mask,
            ..Default::default()
        },
        &mut batched.0,
        &mut batched.1,
    )
    .unwrap();
    assert_eq!(one, inactive, "gs single");
    assert!(problems(&batched).all(|b| b == one), "gs batch");

    let mut one = dirty.clone();
    let mut batched = batch(&dirty, owned.len());
    gspat(
        &NalgebraBackend,
        &g,
        &owned[0],
        wl(),
        &GspatOption {
            mask,
            ..Default::default()
        },
        &mut one.0,
        &mut one.1,
    )
    .unwrap();
    gspat_batch(
        &NalgebraBackend,
        &g,
        &foci,
        wl(),
        &GspatOption {
            mask,
            ..Default::default()
        },
        &mut batched.0,
        &mut batched.1,
    )
    .unwrap();
    assert_eq!(one, inactive, "gspat single");
    assert!(problems(&batched).all(|b| b == one), "gspat batch");
}

#[test]
fn rejects_malformed_batches() {
    let g = geometry(1);
    let foci = problem(&g, 0, 5);
    let mut dst = batch(&slot(&g), 2);

    assert_eq!(
        gs_batch(
            &NalgebraBackend,
            &g,
            &foci,
            wl(),
            &GsOption::default(),
            &mut dst.0,
            &mut dst.1
        ),
        Err(HoloError::BatchSizeMismatch {
            foci: 5,
            problems: 2
        })
    );
    assert_eq!(
        gs_batch(
            &NalgebraBackend,
            &g,
            &foci,
            wl(),
            &GsOption::default(),
            &mut [],
            &mut []
        ),
        Err(HoloError::NoProblems)
    );
    assert_eq!(
        gs_batch(
            &NalgebraBackend,
            &g,
            &[],
            wl(),
            &GsOption::default(),
            &mut [slot(&g).0],
            &mut [slot(&g).1]
        ),
        Err(HoloError::NoFoci)
    );
    let mut two = batch(&slot(&g), 2);
    assert_eq!(
        gs_batch(
            &NalgebraBackend,
            &g,
            &problem(&g, 0, 2),
            wl(),
            &GsOption::default(),
            &mut two.0,
            &mut two.1[..1]
        ),
        Err(HoloError::DstProblemCountMismatch {
            phases: 2,
            intensities: 1
        })
    );
}
