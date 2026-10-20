use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};

use autd3_rs_core::common::Angle;
use autd3_rs_core::value::Phase;

const COUNTS: &[usize] = &[249, 249 * 64];

fn bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("phase");
    for &n in COUNTS {
        let angles: Vec<Angle> = (0..n)
            .map(|i| Angle::from_rad(i as f32 * 0.0137 - 100.0))
            .collect();
        let mut dst = vec![Phase::ZERO; n];
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::new("from_angle", n), &angles, |b, angles| {
            b.iter(|| {
                for (d, &a) in dst.iter_mut().zip(black_box(angles)) {
                    *d = Phase::from(a);
                }
                black_box(&mut dst);
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
