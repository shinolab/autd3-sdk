use std::time::Duration;

use autd3_rs::commands::{FixedCompletionTime, Modulation, Pattern, SetSilencer};
use autd3_rs::common::ULTRASOUND_PERIOD;
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::value::{Intensity, Phase, SamplingConfig};

use autd3_rs_emulator::{
    AxisOrder, Emulator, EmulatorError, Grid, GridAxis, InstantRecordOption, InvalidGrid, Record,
    RmsRecordOption,
};

fn recorded() -> Record {
    let emulator = Emulator::new(Geometry::new(vec![Autd3::default()]));
    let phases = vec![vec![Phase::ZERO; Autd3::NUM_TRANSDUCERS]];
    let intensities = vec![vec![Intensity::MAX; Autd3::NUM_TRANSDUCERS]];
    let modulation = vec![0xFF, 0xFF];

    emulator
        .record(|r| {
            r.send(SetSilencer::new(FixedCompletionTime {
                intensity: ULTRASOUND_PERIOD,
                phase: ULTRASOUND_PERIOD,
                strict_mode: false,
            }))?;
            r.send(Modulation::new(SamplingConfig::FREQ_4K, &modulation))?;
            r.send(Pattern::new(&phases, &intensities))?;
            r.tick(2 * ULTRASOUND_PERIOD)?;
            Ok(())
        })
        .unwrap()
}

fn range() -> Grid {
    Grid {
        x: -10.0..=10.0,
        y: -10.0..=10.0,
        z: 150.0..=150.0,
        resolution: 10.0,
        order: AxisOrder::XYZ,
    }
}

fn invalid_ranges() -> Vec<(Grid, InvalidGrid)> {
    let mut cases: Vec<_> = [0.0, -10.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY]
        .into_iter()
        .map(|resolution| {
            (
                Grid {
                    resolution,
                    ..range()
                },
                InvalidGrid::Resolution,
            )
        })
        .collect();
    cases.push((
        Grid {
            y: 10.0..=-10.0,
            ..range()
        },
        InvalidGrid::Reversed(GridAxis::Y),
    ));
    cases.push((
        Grid {
            z: 150.0..=f32::INFINITY,
            ..range()
        },
        InvalidGrid::NonFiniteBound(GridAxis::Z),
    ));
    cases
}

fn assert_invalid_grid<T>(result: Result<T, EmulatorError>, expected: InvalidGrid) {
    match result {
        Err(EmulatorError::InvalidGrid(e)) => assert_eq!(e, expected),
        Err(e) => panic!("{expected:?}: {e}"),
        Ok(_) => panic!("{expected:?}: accepted"),
    }
}

#[test]
fn sound_field_rejects_an_invalid_grid() {
    let record = recorded();
    for (grid, expected) in invalid_ranges() {
        assert_invalid_grid(
            record.sound_field(grid.clone(), RmsRecordOption::default()),
            expected,
        );
        assert_invalid_grid(
            record.sound_field(grid, InstantRecordOption::default()),
            expected,
        );
    }
}

#[test]
fn single_point_grid_yields_one_observation_point() {
    let record = recorded();
    let grid = Grid {
        x: 0.0..=0.0,
        y: 0.0..=0.0,
        ..range()
    };
    let rms = record
        .sound_field(grid.clone(), RmsRecordOption::default())
        .unwrap();
    assert_eq!(rms.observe_points().rows, 1);

    let mut instant = record
        .sound_field(
            grid,
            InstantRecordOption {
                time_step: Duration::from_micros(5),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(instant.observe_points().rows, 1);
    assert_eq!(instant.next(ULTRASOUND_PERIOD).unwrap().rows, 1);
}

#[test]
fn rms_field_shape() {
    let record = recorded();
    let mut rms = record
        .sound_field(range(), RmsRecordOption::default())
        .unwrap();
    assert_eq!(rms.observe_points().rows, 9);
    let field = rms.next(ULTRASOUND_PERIOD).unwrap();
    assert_eq!(field.rows, 9);
    assert_eq!(field.columns.len(), 1);
}

#[test]
fn instant_field_shape() {
    let record = recorded();
    let option = InstantRecordOption {
        time_step: Duration::from_micros(5),
        ..Default::default()
    };
    let mut instant = record.sound_field(range(), option).unwrap();
    assert_eq!(instant.observe_points().rows, 9);
    let field = instant.next(ULTRASOUND_PERIOD).unwrap();
    assert_eq!(field.rows, 9);
    assert_eq!(field.columns.len(), 5);
}

#[test]
fn rms_skip_then_exhaust_errors() {
    let record = recorded();
    let mut rms = record
        .sound_field(range(), RmsRecordOption::default())
        .unwrap();
    rms.skip(ULTRASOUND_PERIOD).unwrap();
    rms.next(ULTRASOUND_PERIOD).unwrap();
    assert!(rms.next(ULTRASOUND_PERIOD).is_err());
}
