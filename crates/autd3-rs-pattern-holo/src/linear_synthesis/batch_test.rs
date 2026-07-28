#![cfg(test)]

use autd3_rs_core::common::units::{m, s};
use autd3_rs_core::geometry::{Autd3, Geometry, Point3, UnitQuaternion, Vector3};
use autd3_rs_core::value::{Emission, Intensity, Phase};

use crate::amp::Pa;
use crate::backend::NalgebraBackend;
use crate::control_point::ControlPoint;
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

fn problem(g: &Geometry, seed: usize, nf: usize) -> Vec<ControlPoint> {
    (0..nf)
        .map(|i| ControlPoint {
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

fn slot(g: &Geometry) -> Vec<Vec<Emission>> {
    vec![vec![Emission::default(); Autd3::NUM_TRANSDUCERS]; g.num_devices()]
}

fn wl() -> autd3_rs_core::common::Length {
    autd3_rs_pattern::wavelength(340.0 * m / s)
}

#[test]
fn batch_matches_sequential() {
    let g = geometry(2);
    for nf in [1usize, 4] {
        let owned: Vec<Vec<ControlPoint>> = (0..5).map(|k| problem(&g, k, nf)).collect();
        let foci: Vec<&[ControlPoint]> = owned.iter().map(Vec::as_slice).collect();

        let mut batched = vec![slot(&g); foci.len()];
        let mut one = slot(&g);

        naive_batch(
            &NalgebraBackend,
            &g,
            &foci,
            wl(),
            &NaiveOption::default(),
            &mut batched,
        )
        .unwrap();
        for (f, want) in foci.iter().zip(&batched) {
            naive(
                &NalgebraBackend,
                &g,
                f,
                wl(),
                &NaiveOption::default(),
                &mut one,
            )
            .unwrap();
            assert_eq!(&one, want, "naive {nf} foci");
        }

        gs_batch(
            &NalgebraBackend,
            &g,
            &foci,
            wl(),
            &GsOption::default(),
            &mut batched,
        )
        .unwrap();
        for (f, want) in foci.iter().zip(&batched) {
            gs(
                &NalgebraBackend,
                &g,
                f,
                wl(),
                &GsOption::default(),
                &mut one,
            )
            .unwrap();
            assert_eq!(&one, want, "gs {nf} foci");
        }

        gspat_batch(
            &NalgebraBackend,
            &g,
            &foci,
            wl(),
            &GspatOption::default(),
            &mut batched,
        )
        .unwrap();
        for (f, want) in foci.iter().zip(&batched) {
            gspat(
                &NalgebraBackend,
                &g,
                f,
                wl(),
                &GspatOption::default(),
                &mut one,
            )
            .unwrap();
            assert_eq!(&one, want, "gspat {nf} foci");
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
    let owned: Vec<Vec<ControlPoint>> = (0..3).map(|k| problem(&g, k, 4)).collect();
    let foci: Vec<&[ControlPoint]> = owned.iter().map(Vec::as_slice).collect();

    for mask in [TransducerMask::AllEnabled, TransducerMask::Masked(&masked)] {
        let mut on = slot(&g);
        let mut off = slot(&g);
        gs(
            &NalgebraBackend,
            &g,
            foci[0],
            wl(),
            &GsOption {
                mask,
                parallel: true,
                ..Default::default()
            },
            &mut on,
        )
        .unwrap();
        gs(
            &NalgebraBackend,
            &g,
            foci[0],
            wl(),
            &GsOption {
                mask,
                parallel: false,
                ..Default::default()
            },
            &mut off,
        )
        .unwrap();
        assert_eq!(on, off, "single problem");

        let mut on = vec![slot(&g); foci.len()];
        let mut off = vec![slot(&g); foci.len()];
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
            &mut on,
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
            &mut off,
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
    let owned: Vec<Vec<ControlPoint>> = (0..3).map(|k| problem(&g, k, 2)).collect();
    let foci: Vec<&[ControlPoint]> = owned.iter().map(Vec::as_slice).collect();

    let dirty = vec![
        vec![
            Emission {
                phase: Phase(0x7F),
                intensity: Intensity(0xFF),
            };
            Autd3::NUM_TRANSDUCERS
        ];
        g.num_devices()
    ];
    let inactive = slot(&g);

    let mut one = dirty.clone();
    let mut batched = vec![dirty.clone(); foci.len()];
    naive(
        &NalgebraBackend,
        &g,
        foci[0],
        wl(),
        &NaiveOption {
            mask,
            ..Default::default()
        },
        &mut one,
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
        &mut batched,
    )
    .unwrap();
    assert_eq!(one, inactive, "naive single");
    assert!(batched.iter().all(|b| *b == one), "naive batch");

    let mut one = dirty.clone();
    let mut batched = vec![dirty.clone(); foci.len()];
    gs(
        &NalgebraBackend,
        &g,
        foci[0],
        wl(),
        &GsOption {
            mask,
            ..Default::default()
        },
        &mut one,
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
        &mut batched,
    )
    .unwrap();
    assert_eq!(one, inactive, "gs single");
    assert!(batched.iter().all(|b| *b == one), "gs batch");

    let mut one = dirty.clone();
    let mut batched = vec![dirty; foci.len()];
    gspat(
        &NalgebraBackend,
        &g,
        foci[0],
        wl(),
        &GspatOption {
            mask,
            ..Default::default()
        },
        &mut one,
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
        &mut batched,
    )
    .unwrap();
    assert_eq!(one, inactive, "gspat single");
    assert!(batched.iter().all(|b| *b == one), "gspat batch");
}

#[test]
fn rejects_malformed_batches() {
    let g = geometry(1);
    let a = problem(&g, 0, 2);
    let b = problem(&g, 1, 3);
    let mut dst = vec![slot(&g); 2];

    assert_eq!(
        gs_batch(
            &NalgebraBackend,
            &g,
            &[&a, &b],
            wl(),
            &GsOption::default(),
            &mut dst
        ),
        Err(HoloError::UnevenBatch(2, 3))
    );
    assert_eq!(
        gs_batch(
            &NalgebraBackend,
            &g,
            &[&a],
            wl(),
            &GsOption::default(),
            &mut dst
        ),
        Err(HoloError::BatchSizeMismatch {
            problems: 1,
            slots: 2
        })
    );
    assert_eq!(
        gs_batch(
            &NalgebraBackend,
            &g,
            &[],
            wl(),
            &GsOption::default(),
            &mut []
        ),
        Err(HoloError::NoProblems)
    );
    assert_eq!(
        gs_batch(
            &NalgebraBackend,
            &g,
            &[&[]],
            wl(),
            &GsOption::default(),
            &mut [slot(&g)]
        ),
        Err(HoloError::NoFoci)
    );
}
