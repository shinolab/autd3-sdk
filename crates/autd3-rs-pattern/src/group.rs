use autd3_rs_core::geometry::{Device, Geometry, TransducerGroups, TransducerMask};
use autd3_rs_core::value::{Intensity, Phase};

fn sources_of<K, S, F>(groups: &TransducerGroups<K>, mut source: F) -> Vec<S>
where
    K: Copy + Eq,
    F: FnMut(K) -> S,
{
    groups.keys().iter().map(|&key| source(key)).collect()
}

fn write_device<K, T, S>(
    device: &Device,
    groups: &TransducerGroups<K>,
    sources: &[S],
    dst: &mut [T],
) where
    K: Copy + Eq,
    T: Copy,
    S: AsRef<[Vec<T>]>,
{
    let dev = device.idx();
    for (tr, (slot, &index)) in dst.iter_mut().zip(groups.indices(dev)).enumerate() {
        *slot = sources[index].as_ref()[dev][tr];
    }
}

pub fn group_device<K, T, S, F>(
    device: &Device,
    groups: &TransducerGroups<K>,
    source: F,
    dst: &mut [T],
) where
    K: Copy + Eq,
    T: Copy,
    S: AsRef<[Vec<T>]>,
    F: FnMut(K) -> S,
{
    let sources = sources_of(groups, source);
    write_device(device, groups, &sources, dst);
}

pub fn group<K, T, S, F>(
    geometry: &Geometry,
    groups: &TransducerGroups<K>,
    source: F,
    dst: &mut [Vec<T>],
) where
    K: Copy + Eq,
    T: Copy,
    S: AsRef<[Vec<T>]>,
    F: FnMut(K) -> S,
{
    let sources = sources_of(groups, source);
    for (device, slot) in geometry.iter().zip(dst.iter_mut()) {
        write_device(device, groups, &sources, slot);
    }
}

pub fn group_compute<K, E, F>(
    geometry: &Geometry,
    groups: &TransducerGroups<K>,
    mut compute: F,
    phases: &mut [Vec<Phase>],
    intensities: &mut [Vec<Intensity>],
) -> Result<(), E>
where
    K: Copy + Eq,
    F: FnMut(K, TransducerMask<'_>, &mut [Vec<Phase>], &mut [Vec<Intensity>]) -> Result<(), E>,
{
    let mut scratch_phases = geometry.phase_buffer();
    let mut scratch_intensities = geometry.intensity_buffer();
    for (index, (key, mask)) in groups.masks().enumerate() {
        for slot in &mut scratch_phases {
            slot.fill(Phase::ZERO);
        }
        for slot in &mut scratch_intensities {
            slot.fill(Intensity::MAX);
        }
        compute(key, mask, &mut scratch_phases, &mut scratch_intensities)?;
        assert_same_shape(&scratch_phases, phases);
        assert_same_shape(&scratch_intensities, intensities);
        copy_group(groups, index, &scratch_phases, phases);
        copy_group(groups, index, &scratch_intensities, intensities);
    }
    Ok(())
}

fn assert_same_shape<A, B>(scratch: &[Vec<A>], dst: &[Vec<B>]) {
    assert!(
        scratch.len() == dst.len()
            && scratch
                .iter()
                .zip(dst.iter())
                .all(|(source, slot)| source.len() == slot.len()),
        "scratch must have the same shape as dst"
    );
}

#[inline(never)]
fn copy_group<K: Copy + Eq, T: Copy>(
    groups: &TransducerGroups<K>,
    index: usize,
    scratch: &[Vec<T>],
    dst: &mut [Vec<T>],
) {
    for (dev, (slot, source)) in dst.iter_mut().zip(scratch.iter()).enumerate() {
        for ((out, &v), &i) in slot.iter_mut().zip(source).zip(groups.indices(dev)) {
            if i == index {
                *out = v;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use autd3_rs_core::geometry::{Autd3, Point3, UnitQuaternion};

    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Side {
        Left,
        Right,
    }

    fn fill<T: Copy>(value: T, dst: &mut [Vec<T>]) {
        for slot in dst {
            slot.fill(value);
        }
    }

    fn geometry() -> Geometry {
        Geometry::new(vec![
            Autd3::new(Point3::origin(), UnitQuaternion::identity()),
            Autd3::new(
                Point3::new(Autd3::DEVICE_WIDTH, 0.0, 0.0),
                UnitQuaternion::identity(),
            ),
        ])
    }

    fn side_of(dev: usize, tr: usize) -> Side {
        match (dev, tr % 3) {
            (1, 1) => Side::Right,
            _ => Side::Left,
        }
    }

    fn sides(geometry: &Geometry) -> TransducerGroups<Side> {
        TransducerGroups::new(geometry, |device, tr| side_of(device.idx(), tr))
    }

    fn assert_sides<T: Copy + PartialEq + core::fmt::Debug>(dst: &[Vec<T>], left: T, right: T) {
        for (dev, slot) in dst.iter().enumerate() {
            for (tr, &v) in slot.iter().enumerate() {
                let expected = match side_of(dev, tr) {
                    Side::Left => left,
                    Side::Right => right,
                };
                assert_eq!(v, expected, "dev {dev} tr {tr}");
            }
        }
    }

    #[test]
    fn group_copies_the_source_of_each_key() {
        let geometry = geometry();
        let mut left = geometry.phase_buffer();
        fill(Phase(0x10), &mut left);
        let mut right = geometry.phase_buffer();
        fill(Phase(0x30), &mut right);
        let mut dst = geometry.phase_buffer();
        fill(Phase(0xFF), &mut dst);

        let groups = sides(&geometry);
        group(
            &geometry,
            &groups,
            |side| match side {
                Side::Left => &left,
                Side::Right => &right,
            },
            &mut dst,
        );

        assert_sides(&dst, Phase(0x10), Phase(0x30));
    }

    #[test]
    fn group_copies_intensity_buffers_too() {
        let geometry = geometry();
        let mut left = geometry.intensity_buffer();
        fill(Intensity(0x20), &mut left);
        let mut right = geometry.intensity_buffer();
        fill(Intensity(0x40), &mut right);
        let mut dst = geometry.intensity_buffer();
        fill(Intensity(0x60), &mut dst);

        let groups = sides(&geometry);
        group(
            &geometry,
            &groups,
            |side| match side {
                Side::Left => &left,
                Side::Right => &right,
            },
            &mut dst,
        );

        assert_sides(&dst, Intensity(0x20), Intensity(0x40));
    }

    #[test]
    fn group_asks_for_each_source_once_and_reads_the_same_transducer() {
        let geometry = geometry();
        let mut src = geometry.phase_buffer();
        for slot in &mut src {
            for (tr, p) in slot.iter_mut().enumerate() {
                *p = Phase(u8::try_from(tr % 256).unwrap());
            }
        }
        let mut dst = geometry.phase_buffer();
        let groups = TransducerGroups::new(&geometry, |_, _| ());

        let mut calls = 0;
        group(
            &geometry,
            &groups,
            |()| {
                calls += 1;
                &src
            },
            &mut dst,
        );

        assert_eq!(calls, 1);
        assert_eq!(dst, src);
    }

    #[test]
    fn group_device_matches_group() {
        let geometry = geometry();
        let mut src = geometry.phase_buffer();
        fill(Phase(0x55), &mut src);
        let groups = sides(&geometry);

        let mut expected = geometry.phase_buffer();
        fill(Phase(0xFF), &mut expected);
        group(&geometry, &groups, |_| &src, &mut expected);

        let mut dst = vec![Phase(0xFF); Autd3::NUM_TRANSDUCERS];
        group_device(&geometry[1], &groups, |_| &src, &mut dst);
        assert_eq!(dst, expected[1]);
    }

    #[test]
    fn group_compute_passes_the_mask_of_each_key_and_copies_its_transducers() {
        let geometry = geometry();
        let groups = sides(&geometry);
        let mut phases = geometry.phase_buffer();
        fill(Phase(0xFF), &mut phases);
        let mut intensities = geometry.intensity_buffer();
        fill(Intensity(0x60), &mut intensities);

        let mut seen = Vec::new();
        let result: Result<(), ()> = group_compute(
            &geometry,
            &groups,
            |side, mask, phases, intensities| {
                seen.push(side);
                for (dev, slot) in phases.iter().enumerate() {
                    for tr in 0..slot.len() {
                        assert_eq!(mask.is_enabled(dev, tr), groups.key(dev, tr) == side);
                    }
                }
                let (p, i) = match side {
                    Side::Left => (Phase(0x10), Intensity(0x20)),
                    Side::Right => (Phase(0x30), Intensity(0x40)),
                };
                fill(p, phases);
                fill(i, intensities);
                Ok(())
            },
            &mut phases,
            &mut intensities,
        );

        assert_eq!(result, Ok(()));
        assert_eq!(seen, [Side::Left, Side::Right]);
        assert_sides(&phases, Phase(0x10), Phase(0x30));
        assert_sides(&intensities, Intensity(0x20), Intensity(0x40));
    }

    #[test]
    fn group_compute_stops_at_the_first_error() {
        let geometry = geometry();
        let groups = sides(&geometry);
        let mut phases = geometry.phase_buffer();
        let mut intensities = geometry.intensity_buffer();

        let mut calls = 0;
        let result = group_compute(
            &geometry,
            &groups,
            |side, _, _, _| {
                calls += 1;
                if side == Side::Left {
                    Err("left failed")
                } else {
                    Ok(())
                }
            },
            &mut phases,
            &mut intensities,
        );

        assert_eq!(result, Err("left failed"));
        assert_eq!(calls, 1);
    }

    #[test]
    fn group_compute_hands_a_fresh_pattern_buffer_to_each_key() {
        let geometry = geometry();
        let groups = sides(&geometry);
        let mut phases = geometry.phase_buffer();
        fill(Phase(0xFF), &mut phases);
        let mut intensities = geometry.intensity_buffer();
        fill(Intensity(0x60), &mut intensities);

        let result: Result<(), ()> = group_compute(
            &geometry,
            &groups,
            |side, _, phases, intensities| {
                if side == Side::Left {
                    fill(Phase(0x10), phases);
                    fill(Intensity(0x20), intensities);
                }
                Ok(())
            },
            &mut phases,
            &mut intensities,
        );

        assert_eq!(result, Ok(()));
        assert_sides(&phases, Phase(0x10), Phase::ZERO);
        assert_sides(&intensities, Intensity(0x20), Intensity::MAX);
    }
}
