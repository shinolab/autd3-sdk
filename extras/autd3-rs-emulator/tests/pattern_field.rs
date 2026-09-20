use autd3_rs::commands::{FixedCompletionTime, Modulation, Pattern, SetSilencer};
use autd3_rs::common::ULTRASOUND_PERIOD;
use autd3_rs::geometry::{Autd3, Geometry, Point3, UnitVector3, Vector3};
use autd3_rs::units::{m, mm, s};
use autd3_rs::value::{Intensity, Phase, SamplingConfig};
use autd3_rs_emulator::{ClientApi, Emulator, RangeXY, RawColumn, Record, RmsRecordOption};
use autd3_rs_pattern::{
    HermiteGaussianOption, LaguerreGaussianOption, focus, hermite_gaussian_intensity,
    hermite_gaussian_phase, laguerre_gaussian_intensity, laguerre_gaussian_phase, wavelength,
};

const HALF_SPAN: f32 = 10.0;
const RESOLUTION: f32 = 0.5;
const GRID: usize = 41;
const CENTER: usize = GRID / 2;

fn geometry() -> Geometry {
    Geometry::new(vec![Autd3::default()])
}

fn record(geometry: Geometry, phases: Vec<Vec<Phase>>, intensities: Vec<Vec<Intensity>>) -> Record {
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
                .push(Pattern::new(&phases, &intensities));
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

type Setup<'a> = dyn FnOnce(&Geometry, Point3<f32>, &mut [Vec<Phase>], &mut [Vec<Intensity>]) + 'a;

fn setup(f: Box<Setup<'_>>) -> Vec<f32> {
    let geometry = geometry();
    let target = geometry.center() + Vector3::new(0.0, 0.0, 150.0);
    let mut phases = geometry.phase_buffer();
    let mut intensities = geometry.intensity_buffer();
    f(&geometry, target, &mut phases, &mut intensities);
    let record = record(geometry, phases, intensities);
    rms_grid(&record, target)
}

#[test]
fn focus_peaks_on_axis() {
    let grid = setup(Box::new(|geometry, target, phases, _| {
        focus(geometry, target, wavelength(340.0 * m / s), phases);
    }));

    assert_eq!(argmax(&grid), (CENTER, CENTER));
    assert!(at(&grid, CENTER, CENTER) > 0.9 * max_of(&grid));
}

fn laguerre_gaussian_grid(p: u32, l: i32) -> Vec<f32> {
    setup(Box::new(move |geometry, target, phases, intensities| {
        let option = LaguerreGaussianOption {
            p,
            l,
            waist: 10.0 * mm,
        };
        let lambda = wavelength(340.0 * m / s);
        laguerre_gaussian_phase(geometry, target, Vector3::z_axis(), option, lambda, phases);
        laguerre_gaussian_intensity(
            geometry,
            target,
            Vector3::z_axis(),
            option,
            lambda,
            intensities,
        );
    }))
}

#[test]
fn laguerre_gaussian_fundamental_is_a_focus() {
    let grid = laguerre_gaussian_grid(0, 0);

    assert_eq!(argmax(&grid), (CENTER, CENTER));
}

#[test]
fn laguerre_gaussian_vortex_mode_is_a_ring_with_a_null_on_axis() {
    let grid = laguerre_gaussian_grid(0, 1);

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
        "the ring peak must sit off the axis, got it {step} cells from the center"
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
}

#[test]
fn laguerre_gaussian_radial_mode_has_a_dark_ring_around_the_peak() {
    let grid = laguerre_gaussian_grid(1, 0);

    let center = at(&grid, CENTER, CENTER);
    assert_eq!(argmax(&grid), (CENTER, CENTER));
    let row: Vec<f32> = (CENTER..GRID).map(|ix| at(&grid, ix, CENTER)).collect();
    let dark = row
        .iter()
        .enumerate()
        .min_by(|a, b| a.1.total_cmp(b.1))
        .unwrap();
    let bright_outside = row[dark.0..].iter().copied().fold(f32::MIN, f32::max);
    assert!(
        *dark.1 < 0.3 * center && bright_outside > 2.0 * dark.1,
        "expected a dark ring between the core and an outer ring, got {row:?}"
    );
}

fn hermite_gaussian_grid(x_dir: UnitVector3<f32>) -> Vec<f32> {
    setup(Box::new(move |geometry, target, phases, intensities| {
        let option = HermiteGaussianOption {
            m: 1,
            n: 0,
            waist: 10.0 * mm,
        };
        let lambda = wavelength(340.0 * m / s);
        let axis = Vector3::z_axis();
        hermite_gaussian_phase(geometry, target, axis, x_dir, option, lambda, phases);
        hermite_gaussian_intensity(geometry, target, axis, x_dir, option, lambda, intensities);
    }))
}

#[test]
fn hermite_gaussian_first_order_has_two_lobes_across_the_node_plane() {
    let grid = hermite_gaussian_grid(Vector3::x_axis());

    let peak = max_of(&grid);
    let on_axis = at(&grid, CENTER, CENTER);
    assert!(
        on_axis < 0.1 * peak,
        "the node plane must be dark, got {on_axis} against {peak}"
    );
    let row: Vec<f32> = (0..GRID).map(|ix| at(&grid, ix, CENTER)).collect();
    let left = row[..CENTER].iter().copied().fold(f32::MIN, f32::max);
    let right = row[CENTER + 1..].iter().copied().fold(f32::MIN, f32::max);
    assert!(
        left.min(right) > 0.8 * left.max(right) && left.min(right) > 0.9 * peak,
        "the two lobes must sit along x, got left {left} right {right} peak {peak}"
    );
    let column: Vec<f32> = (0..GRID).map(|iy| at(&grid, CENTER, iy)).collect();
    let along_node = column.iter().copied().fold(f32::MIN, f32::max);
    assert!(
        along_node < 0.5 * peak,
        "the node plane must stay dark, got {along_node} against {peak}"
    );
}

#[test]
fn hermite_gaussian_x_dir_rotates_the_lobes() {
    let grid = hermite_gaussian_grid(Vector3::y_axis());

    let peak = max_of(&grid);
    let row: Vec<f32> = (0..GRID).map(|ix| at(&grid, ix, CENTER)).collect();
    let column: Vec<f32> = (0..GRID).map(|iy| at(&grid, CENTER, iy)).collect();
    let along_node = row.iter().copied().fold(f32::MIN, f32::max);
    let across_node = column.iter().copied().fold(f32::MIN, f32::max);
    assert!(
        across_node > 0.9 * peak && along_node < 0.5 * peak,
        "with x_dir = y the lobes must sit along y, got across {across_node} along {along_node}"
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
