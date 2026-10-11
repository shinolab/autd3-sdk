use std::time::Duration;

use autd3_rs::commands::{FixedCompletionTime, Modulation, Pattern, SetSilencer};
use autd3_rs::common::ULTRASOUND_PERIOD;
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::value::{Intensity, Phase, SamplingConfig};

use autd3_rs_emulator::{
    AxisOrder, ClientApi, Emulator, EmulatorError, Grid, GridAxis, InstantRecordOption,
    InvalidGrid, Record, RmsRecordOption,
};

fn recorded() -> Record {
    let emulator = Emulator::new(Geometry::new(vec![Autd3::default()]));
    let phases = vec![vec![Phase::ZERO; Autd3::NUM_TRANSDUCERS]];
    let intensities = vec![vec![Intensity::MAX; Autd3::NUM_TRANSDUCERS]];
    let modulation = vec![0xFF, 0xFF];

    emulator
        .record(async move |r| {
            r.send(SetSilencer::new(FixedCompletionTime {
                intensity: ULTRASOUND_PERIOD,
                phase: ULTRASOUND_PERIOD,
                strict_mode: false,
            }))
            .await?;
            r.send(Modulation::new(SamplingConfig::FREQ_4K, &modulation))
                .await?;
            r.send(Pattern::new(&phases, &intensities)).await?;
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
    assert_eq!(rms.observe_points().height(), 1);

    let mut instant = record
        .sound_field(
            grid,
            InstantRecordOption {
                time_step: Duration::from_micros(5),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(instant.observe_points().height(), 1);
    assert_eq!(instant.next(ULTRASOUND_PERIOD).unwrap().height(), 1);
}

#[test]
fn rms_field_shape() {
    let record = recorded();
    let mut rms = record
        .sound_field(range(), RmsRecordOption::default())
        .unwrap();
    assert_eq!(rms.observe_points().height(), 9);
    let field = rms.next(ULTRASOUND_PERIOD).unwrap();
    assert_eq!(field.height(), 9);
    assert_eq!(field.width(), 1);
}

#[test]
fn instant_field_shape() {
    let record = recorded();
    let option = InstantRecordOption {
        time_step: Duration::from_micros(5),
        ..Default::default()
    };
    let mut instant = record.sound_field(range(), option).unwrap();
    assert_eq!(instant.observe_points().height(), 9);
    let field = instant.next(ULTRASOUND_PERIOD).unwrap();
    assert_eq!(field.height(), 9);
    assert_eq!(field.width(), 5);
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

#[cfg(feature = "gpu")]
mod gpu {
    use super::{ULTRASOUND_PERIOD, assert_invalid_grid, invalid_ranges, range, recorded};
    use autd3_rs_emulator::{EmulatorError, InstantRecordOption, RmsRecordOption};
    use polars::frame::DataFrame;
    use std::time::Duration;

    fn is_missing_adapter(e: &EmulatorError) -> bool {
        matches!(
            e,
            EmulatorError::RequestAdapter(_) | EmulatorError::RequestDevice(_)
        )
    }

    fn assert_fields_close(cpu: &DataFrame, gpu: &DataFrame) {
        assert_eq!(cpu.shape(), gpu.shape());
        for (c, g) in cpu.columns().iter().zip(gpu.columns()) {
            for (a, b) in c
                .f32()
                .unwrap()
                .into_no_null_iter()
                .zip(g.f32().unwrap().into_no_null_iter())
            {
                approx::assert_abs_diff_eq!(a, b, epsilon = 0.1);
            }
        }
    }

    fn near_range() -> autd3_rs_emulator::Grid {
        autd3_rs_emulator::Grid {
            x: -10.0..=10.0,
            y: -10.0..=10.0,
            z: 1.0..=1.0,
            resolution: 10.0,
            order: autd3_rs_emulator::AxisOrder::XYZ,
        }
    }

    #[test]
    fn gpu_sound_field_rejects_an_invalid_grid() {
        let record = recorded();
        for (grid, expected) in invalid_ranges() {
            assert_invalid_grid(
                record.sound_field(
                    grid.clone(),
                    RmsRecordOption {
                        gpu: true,
                        ..Default::default()
                    },
                ),
                expected,
            );
            assert_invalid_grid(
                record.sound_field(
                    grid,
                    InstantRecordOption {
                        gpu: true,
                        ..Default::default()
                    },
                ),
                expected,
            );
        }
    }

    #[test]
    fn rms_gpu_matches_cpu() {
        let record = recorded();
        let cpu = record
            .sound_field(range(), RmsRecordOption::default())
            .unwrap()
            .next(ULTRASOUND_PERIOD)
            .unwrap();
        let mut gpu = match record.sound_field(
            range(),
            RmsRecordOption {
                gpu: true,
                ..Default::default()
            },
        ) {
            Ok(g) => g,
            Err(e) if is_missing_adapter(&e) => {
                eprintln!("skipping rms_gpu_matches_cpu: no GPU adapter ({e})");
                return;
            }
            Err(e) => panic!("{e}"),
        };
        assert_fields_close(&cpu, &gpu.next(ULTRASOUND_PERIOD).unwrap());
    }

    #[test]
    fn instant_gpu_matches_cpu() {
        let record = recorded();
        let option = InstantRecordOption {
            time_step: Duration::from_micros(1),
            ..Default::default()
        };
        let cpu = record
            .sound_field(near_range(), option)
            .unwrap()
            .next(ULTRASOUND_PERIOD)
            .unwrap();
        let mut gpu = match record.sound_field(
            near_range(),
            InstantRecordOption {
                gpu: true,
                ..option
            },
        ) {
            Ok(g) => g,
            Err(e) if is_missing_adapter(&e) => {
                eprintln!("skipping instant_gpu_matches_cpu: no GPU adapter ({e})");
                return;
            }
            Err(e) => panic!("{e}"),
        };
        assert_fields_close(&cpu, &gpu.next(ULTRASOUND_PERIOD).unwrap());
    }
}
