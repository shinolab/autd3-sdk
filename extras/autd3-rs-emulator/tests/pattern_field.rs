use autd3_rs::commands::{FixedCompletionTime, Modulation, Pattern, SetSilencer};
use autd3_rs::common::ULTRASOUND_PERIOD;
use autd3_rs::geometry::{Autd3, Geometry, Point3, Vector3};
use autd3_rs::units::{m, mm, s};
use autd3_rs::value::{Emission, SamplingConfig};
use autd3_rs_emulator::{ClientApi, Emulator, RangeXY, RawColumn, Record, RmsRecordOption};
use autd3_rs_pattern::{
    FocusOption, TwinTrapOption, VortexOption, focus, twin_trap, vortex, wavelength,
};

const HALF_SPAN: f32 = 10.0;
const RESOLUTION: f32 = 0.5;
const GRID: usize = 41;
const CENTER: usize = GRID / 2;

fn geometry() -> Geometry {
    Geometry::new(vec![Autd3::default()])
}

fn record(geometry: Geometry, emissions: Vec<Vec<Emission>>) -> Record {
    let emulator = Emulator::new(geometry);
    let modulation = vec![0xFF, 0xFF];
    emulator
        .record(async move |r| {
            let mut builder = r.datagram_builder();
            builder
                .push(SetSilencer {
                    config: FixedCompletionTime {
                        intensity: ULTRASOUND_PERIOD,
                        phase: ULTRASOUND_PERIOD,
                        strict_mode: false,
                    },
                })
                .push(Modulation::new(SamplingConfig::FREQ_4K, &modulation))
                .push(Pattern::new(&emissions));
            let datagrams = builder.build()?;
            for frame in &datagrams {
                r.send_checked(frame).await?;
            }
            r.tick(4 * ULTRASOUND_PERIOD)?;
            Ok(())
        })
        .unwrap()
}

fn rms_grid(record: &Record, target: Point3<f32>) -> Vec<f32> {
    let range = RangeXY {
        x: (target.x - HALF_SPAN)..=(target.x + HALF_SPAN),
        y: (target.y - HALF_SPAN)..=(target.y + HALF_SPAN),
        z: target.z,
        resolution: RESOLUTION,
    };
    let mut rms = record
        .sound_field(range, RmsRecordOption::default())
        .unwrap();
    rms.skip(3 * ULTRASOUND_PERIOD).unwrap();
    let frame = rms.next_raw(ULTRASOUND_PERIOD).unwrap();
    assert_eq!(frame.rows, GRID * GRID);
    match &frame.columns[0].1 {
        RawColumn::F32(v) => v.clone(),
        _ => panic!("rms column must be f32"),
    }
}

fn at(grid: &[f32], ix: usize, iy: usize) -> f32 {
    grid[iy * GRID + ix]
}

fn max_of(grid: &[f32]) -> f32 {
    grid.iter().copied().fold(f32::MIN, f32::max)
}

fn argmax(grid: &[f32]) -> (usize, usize) {
    let i = grid
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .unwrap()
        .0;
    (i % GRID, i / GRID)
}

fn setup(f: impl FnOnce(&Geometry, Point3<f32>, &mut Vec<Vec<Emission>>)) -> Vec<f32> {
    let geometry = geometry();
    let target = geometry.center() + Vector3::new(0.0, 0.0, 150.0);
    let mut emissions = geometry.pattern_buffer();
    f(&geometry, target, &mut emissions);
    let record = record(geometry, emissions);
    rms_grid(&record, target)
}

#[test]
fn focus_peaks_on_axis() {
    let grid = setup(|geometry, target, dst| {
        focus(
            geometry,
            target,
            wavelength(340.0 * m / s),
            &FocusOption::default(),
            dst,
        );
    });

    assert_eq!(argmax(&grid), (CENTER, CENTER));
    assert!(at(&grid, CENTER, CENTER) > 0.9 * max_of(&grid));
}

#[test]
fn vortex_is_a_ring_with_a_null_on_axis() {
    let grid = setup(|geometry, target, dst| {
        vortex(
            geometry,
            target,
            Vector3::z_axis(),
            1,
            wavelength(340.0 * m / s),
            &VortexOption::default(),
            dst,
        );
    });

    let peak = max_of(&grid);
    let on_axis = at(&grid, CENTER, CENTER);
    assert!(
        on_axis < 0.1 * peak,
        "on-axis {on_axis} must be a null against the ring peak {peak}"
    );

    let (px, py) = argmax(&grid);
    let step = px.abs_diff(CENTER).max(py.abs_diff(CENTER));
    assert!(
        step >= 3,
        "the vortex peak must sit off the axis, got it {step} cells from the center"
    );
    let ring = [
        at(&grid, CENTER + step, CENTER),
        at(&grid, CENTER - step, CENTER),
        at(&grid, CENTER, CENTER + step),
        at(&grid, CENTER, CENTER - step),
    ];
    let ring_min = ring.iter().copied().fold(f32::MAX, f32::min);
    let ring_max = ring.iter().copied().fold(f32::MIN, f32::max);
    assert!(
        ring_min > 0.7 * ring_max,
        "the ring must be axisymmetric, got {ring:?}"
    );
    assert!(
        ring_min > 3.0 * on_axis,
        "the ring {ring_min} must stand well above the on-axis null {on_axis}"
    );
}

#[test]
fn vortex_order_zero_is_a_focus() {
    let grid = setup(|geometry, target, dst| {
        vortex(
            geometry,
            target,
            Vector3::z_axis(),
            0,
            wavelength(340.0 * m / s),
            &VortexOption::default(),
            dst,
        );
    });

    assert_eq!(argmax(&grid), (CENTER, CENTER));
}

#[test]
fn twin_trap_has_two_lobes_across_a_node() {
    let grid = setup(|geometry, target, dst| {
        twin_trap(
            geometry,
            target,
            Vector3::x_axis(),
            wavelength(340.0 * m / s),
            &TwinTrapOption::default(),
            dst,
        );
    });

    let peak = max_of(&grid);
    let on_axis = at(&grid, CENTER, CENTER);
    assert!(
        on_axis < 0.1 * peak,
        "the split plane must hold a node, got {on_axis} against {peak}"
    );

    let row: Vec<f32> = (0..GRID).map(|ix| at(&grid, ix, CENTER)).collect();
    let left = row[..CENTER].iter().copied().fold(f32::MIN, f32::max);
    let right = row[CENTER + 1..].iter().copied().fold(f32::MIN, f32::max);
    assert!(
        left.min(right) > 0.8 * left.max(right),
        "the two lobes must be balanced, got left {left} right {right}"
    );
    assert!(
        left.min(right) > 0.5 * peak,
        "both lobes must carry the field, got left {left} right {right} peak {peak}"
    );

    let column: Vec<f32> = (0..GRID).map(|iy| at(&grid, CENTER, iy)).collect();
    let along_normal_plane = column.iter().copied().fold(f32::MIN, f32::max);
    assert!(
        along_normal_plane < 0.5 * peak,
        "the split plane must stay dark, got {along_normal_plane} against {peak}"
    );
}

#[test]
fn twin_trap_normal_rotates_the_lobes() {
    let grid = setup(|geometry, target, dst| {
        twin_trap(
            geometry,
            target,
            Vector3::y_axis(),
            wavelength(340.0 * m / s),
            &TwinTrapOption::default(),
            dst,
        );
    });

    let peak = max_of(&grid);
    let row: Vec<f32> = (0..GRID).map(|ix| at(&grid, ix, CENTER)).collect();
    let column: Vec<f32> = (0..GRID).map(|iy| at(&grid, CENTER, iy)).collect();
    let along_split = row.iter().copied().fold(f32::MIN, f32::max);
    let across_split = column.iter().copied().fold(f32::MIN, f32::max);
    assert!(
        across_split > 0.9 * peak && along_split < 0.5 * peak,
        "with normal = y the lobes must sit along y, got across {across_split} along {along_split}"
    );
}

#[test]
fn wavelength_is_8_5_mm_at_340_m_per_s() {
    approx::assert_abs_diff_eq!(
        wavelength(340.0 * m / s).mm(),
        (8.5 * mm).mm(),
        epsilon = 1e-4
    );
}
