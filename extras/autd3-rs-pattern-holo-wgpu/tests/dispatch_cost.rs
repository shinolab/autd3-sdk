use std::time::{Duration, Instant};

use nalgebra::Complex;

use autd3_rs_core::geometry::{Autd3, Geometry, Point3, UnitQuaternion, Vector3};
use autd3_rs_core::value::Intensity;
use autd3_rs_pattern_holo::{
    ControlPoint, Directivity, EmissionConstraint, LinAlgBackend, NalgebraBackend, Pa,
};
use autd3_rs_pattern_holo_wgpu::{GpuMatrix, WgpuBackend};

const N: usize = 249 * 64;
const REPEAT: usize = 200;
const QUANTIZE_ITERS: usize = 20;
const TRIALS: usize = 5;

fn propagation(gpu: &WgpuBackend, foci: usize) -> GpuMatrix {
    use autd3_rs_core::common::units::{m, s};
    let geometry = Geometry::new(
        (0..64)
            .map(|i| {
                Autd3::new(
                    Point3::new(i as f32 * 200.0, 0.0, 0.0),
                    UnitQuaternion::identity(),
                )
            })
            .collect(),
    );
    let mut tr_pos = Vec::with_capacity(N);
    let mut tr_dir = Vec::with_capacity(N);
    for dev in &geometry {
        tr_pos.extend_from_slice(dev.positions());
        tr_dir.extend_from_slice(dev.directions());
    }
    let f: Vec<ControlPoint> = (0..foci)
        .map(|i| ControlPoint {
            point: geometry.center() + Vector3::new(i as f32 * 10.0, 0.0, 150.0),
            amplitude: 5e3 * Pa,
        })
        .collect();
    let wavenumber = 2.0 * core::f32::consts::PI / autd3_rs_pattern::wavelength(340.0 * m / s).mm();
    gpu.propagation_matrix(&tr_pos, &tr_dir, &f, wavenumber, Directivity::Sphere)
}

fn measure(gpu: &WgpuBackend, label: &str, dispatches: usize, mut record: impl FnMut()) {
    let sink = gpu.make_vector(vec![Complex::new(1.0, 0.0); 1]);
    for _ in 0..2 {
        for _ in 0..8 {
            record();
        }
        let _ = gpu.vector_to_host(&sink);
    }
    let mut rec = Duration::MAX;
    let mut exec = Duration::MAX;
    for _ in 0..TRIALS {
        let t = Instant::now();
        for _ in 0..REPEAT {
            record();
        }
        rec = rec.min(t.elapsed());
        let t = Instant::now();
        let _ = gpu.vector_to_host(&sink);
        exec = exec.min(t.elapsed());
    }
    let total = dispatches * REPEAT;
    println!(
        "{label:<28} {:>3} disp/iter  record {:>6.2} us/disp  exec {:>6.2} us/disp  \
         {:>7.2} us/iter",
        dispatches,
        rec.as_secs_f64() * 1e6 / total as f64,
        exec.as_secs_f64() * 1e6 / total as f64,
        (rec + exec).as_secs_f64() * 1e6 / REPEAT as f64,
    );
}

#[test]
#[ignore = "requires a GPU; run explicitly to measure"]
fn dispatch_cost_breakdown() {
    let Ok(gpu) = WgpuBackend::new() else {
        eprintln!("no GPU");
        return;
    };

    {
        let t = Instant::now();
        let mut sink = Vec::with_capacity(2000);
        for _ in 0..2000 {
            sink.push(gpu.make_vector(vec![Complex::new(1.0, 0.0); 4]));
        }
        println!(
            "make_vector (small):        {:>6.2} us/call",
            t.elapsed().as_secs_f64() * 1e6 / 2000.0
        );
        let t = Instant::now();
        drop(sink);
        println!(
            "drop 2000 buffers:          {:>6.2} us/call",
            t.elapsed().as_secs_f64() * 1e6 / 2000.0
        );
    }

    for m in [1usize, 16] {
        println!("--- 64 devices (n={N}) / {m} foci ---");
        let g = propagation(&gpu, m);
        let b = gpu.back_prop(&g);
        let amps = gpu.make_vector(vec![Complex::new(1.0, 0.0); m]);
        let q0 = gpu.make_vector(vec![Complex::new(1.0, 0.0); N]);
        let p0 = gpu.make_vector(vec![Complex::new(1.0, 0.0); m]);

        {
            let mut q = gpu.clone_vector(&q0);
            measure(&gpu, "hadamard_normalize(n)", 1, || {
                gpu.hadamard_normalize(&mut q, &q0);
            });
        }
        {
            let mut p = gpu.clone_vector(&p0);
            measure(&gpu, "hadamard_normalize(m)", 1, || {
                gpu.hadamard_normalize(&mut p, &amps);
            });
        }
        measure(&gpu, "gemv(g) partial", 1, || {
            let _ = gpu.gemv(&g, &q0);
        });
        measure(&gpu, "gemv(b) rowwise", 1, || {
            let _ = gpu.gemv(&b, &p0);
        });
        measure(&gpu, "gemv(g)+gemv(b) folded", 2, || {
            let _ = gpu.gemv(&b, &gpu.gemv(&g, &q0));
        });
        {
            let mut q = gpu.clone_vector(&q0);
            measure(&gpu, "gs iteration (unfused)", 5, || {
                gpu.hadamard_normalize(&mut q, &q0);
                let mut p = gpu.gemv(&g, &q);
                gpu.hadamard_normalize(&mut p, &amps);
                q = gpu.gemv(&b, &p);
            });
        }
        {
            let mut q = Some(gpu.clone_vector(&q0));
            measure(&gpu, "gs iteration (fused)", 2, || {
                let p = gpu.gemv_hadamard_normalized(&g, q.take().unwrap(), &q0);
                q = Some(gpu.gemv_hadamard_normalized(&b, p, &amps));
            });
        }
    }
}

#[test]
#[ignore = "requires a GPU; run explicitly to measure"]
fn quantize_cost() {
    let Ok(gpu) = WgpuBackend::new() else {
        eprintln!("no GPU");
        return;
    };
    println!(
        "{:<10} {:>6} {:>11} {:>11} {:>9} {:>9}",
        "constraint", "batch", "gpu[ms]", "cpu[ms]", "gpu[ns/tr]", "speedup"
    );
    for batch in [1usize, 16, 64] {
        let data = vec![Complex::new(1.0, 0.5); batch * N];
        let gpu_v = gpu.make_batch_vector(batch, data.clone());
        let cpu_v = NalgebraBackend.make_batch_vector(batch, data);
        for (label, c) in [
            (
                "clamp",
                EmissionConstraint::Clamp(Intensity::MIN, Intensity::MAX),
            ),
            ("normalize", EmissionConstraint::Normalize),
        ] {
            let _ = gpu.quantize_batch(&gpu_v, c, false);
            let t = Instant::now();
            for _ in 0..QUANTIZE_ITERS {
                let _ = gpu.quantize_batch(&gpu_v, c, false);
            }
            let gpu_ms = t.elapsed().as_secs_f64() * 1e3 / QUANTIZE_ITERS as f64;

            let _ = NalgebraBackend.quantize_batch(&cpu_v, c, true);
            let t = Instant::now();
            for _ in 0..QUANTIZE_ITERS {
                let _ = NalgebraBackend.quantize_batch(&cpu_v, c, true);
            }
            let cpu_ms = t.elapsed().as_secs_f64() * 1e3 / QUANTIZE_ITERS as f64;

            println!(
                "{label:<10} {batch:>6} {gpu_ms:>11.3} {cpu_ms:>11.3} \
                 {:>10.2} {:>8.2}x",
                gpu_ms * 1e6 / (batch * N) as f64,
                cpu_ms / gpu_ms
            );
        }
    }
}

#[test]
#[ignore = "requires a GPU; run explicitly to measure"]
fn bind_group_reuse() {
    let Ok(gpu) = WgpuBackend::new() else {
        eprintln!("no GPU");
        return;
    };
    for m in [1usize, 16] {
        let g = propagation(&gpu, m);
        let b = gpu.back_prop(&g);
        let amps = gpu.make_vector(vec![Complex::new(1.0, 0.0); m]);
        let q0 = gpu.make_vector(vec![Complex::new(1.0, 0.0); N]);
        let mut q = Some(gpu.clone_vector(&q0));
        let built: Vec<String> = (0..10)
            .map(|_| {
                let before = gpu.bind_groups_created();
                let p = gpu.gemv_hadamard_normalized(&g, q.take().unwrap(), &q0);
                q = Some(gpu.gemv_hadamard_normalized(&b, p, &amps));
                (gpu.bind_groups_created() - before).to_string()
            })
            .collect();
        println!(
            "{m:>3} foci: bind groups built per gs iteration: {}",
            built.join(" ")
        );
    }
}
