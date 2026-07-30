use nalgebra::Complex;

use autd3_rs_core::geometry::{Point3, UnitVector3, Vector3};
use autd3_rs_pattern_holo::{ControlPoint, Directivity, LinAlgBackend, Pa};
use autd3_rs_pattern_holo_wgpu::WgpuBackend;

const WAVENUMBER: f32 = 0.0733;
const M: usize = 8;

fn samples(count: usize, seed: u32) -> Vec<Complex<f32>> {
    let mut state = seed | 1;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        (state >> 8) as f32 / (1 << 24) as f32 - 0.5
    };
    (0..count)
        .map(|_| Complex::new(next() + 0.75, next()))
        .collect()
}

fn bits(v: &[Complex<f32>]) -> Vec<(u32, u32)> {
    v.iter().map(|c| (c.re.to_bits(), c.im.to_bits())).collect()
}

fn transducers(n: usize) -> (Vec<Point3<f32>>, Vec<UnitVector3<f32>>) {
    let pos = (0..n)
        .map(|i| Point3::new((i % 18) as f32 * 10.16, (i / 18) as f32 * 10.16, 0.0))
        .collect();
    let dir = vec![UnitVector3::new_normalize(Vector3::z()); n];
    (pos, dir)
}

fn foci(count: usize) -> Vec<ControlPoint> {
    (0..count)
        .map(|i| ControlPoint {
            point: Point3::new(i as f32 * 7.0, i as f32 * -3.0, 150.0),
            amplitude: (3e3 + i as f32 * 200.0) * Pa,
        })
        .collect()
}

#[test]
fn fused_repeat_matches_the_unfused_chain() {
    let Ok(gpu) = WgpuBackend::new() else {
        eprintln!("skipping: no GPU adapter available");
        return;
    };

    for n in [1usize, 16, 64, 257] {
        for repeat in [0usize, 1, 5, 100] {
            let a = || gpu.make_matrix(n, n, samples(n * n, 0x2545_f491));
            let x = || gpu.make_vector(samples(n, 0x9e37_79b9));
            let r = gpu.make_vector(samples(n, 0x85eb_ca6b));

            let mut want = x();
            for _ in 0..repeat {
                want = gpu.gemv_hadamard_normalized(&a(), want, &r);
            }
            let got = gpu.repeat_gemv_normalized(&a(), x(), &r, repeat);

            assert_eq!(
                bits(&gpu.vector_to_host(&want)),
                bits(&gpu.vector_to_host(&got)),
                "{n}x{n} repeat {repeat}",
            );
        }
    }
}

#[test]
fn fused_repeat_matches_the_unfused_chain_batched() {
    let Ok(gpu) = WgpuBackend::new() else {
        eprintln!("skipping: no GPU adapter available");
        return;
    };

    let (tr_pos, tr_dir) = transducers(249);
    for batch in [1usize, 3] {
        for repeat in [1usize, 7] {
            let g = gpu.batch_propagation_matrix(
                &tr_pos,
                &tr_dir,
                &foci(M * batch),
                batch,
                WAVENUMBER,
                Directivity::Sphere,
            );
            let a = gpu.batch_gemm(&g, &gpu.batch_back_prop(&g));
            let x = || gpu.make_batch_vector(batch, samples(M * batch, 0x9e37_79b9));
            let r = gpu.make_batch_vector(batch, samples(M * batch, 0x85eb_ca6b));

            let mut want = x();
            for _ in 0..repeat {
                want = gpu.batch_gemv_hadamard_normalized(&a, want, &r);
            }
            let got = gpu.batch_repeat_gemv_normalized(&a, x(), &r, repeat);

            assert_eq!(
                bits(&gpu.batch_vector_to_host(&want)),
                bits(&gpu.batch_vector_to_host(&got)),
                "batch {batch} repeat {repeat}",
            );
        }
    }
}
