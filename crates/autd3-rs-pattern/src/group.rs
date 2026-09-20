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
    null: T,
    dst: &mut [T],
) where
    K: Copy + Eq,
    T: Copy,
    S: AsRef<[Vec<T>]>,
{
    let dev = device.idx();
    for (tr, (slot, &index)) in dst.iter_mut().zip(groups.indices(dev)).enumerate() {
        *slot = index.map_or(null, |index| sources[index].as_ref()[dev][tr]);
    }
}

pub fn group_device<K, T, S, F>(
    device: &Device,
    groups: &TransducerGroups<K>,
    source: F,
    null: T,
    dst: &mut [T],
) where
    K: Copy + Eq,
    T: Copy,
    S: AsRef<[Vec<T>]>,
    F: FnMut(K) -> S,
{
    let sources = sources_of(groups, source);
    write_device(device, groups, &sources, null, dst);
}

pub fn group<K, T, S, F>(
    geometry: &Geometry,
    groups: &TransducerGroups<K>,
    source: F,
    null: T,
    dst: &mut [Vec<T>],
) where
    K: Copy + Eq,
    T: Copy,
    S: AsRef<[Vec<T>]>,
    F: FnMut(K) -> S,
{
    let sources = sources_of(groups, source);
    for (device, slot) in geometry.iter().zip(dst.iter_mut()) {
        write_device(device, groups, &sources, null, slot);
    }
}

pub fn group_compute<K, E, F>(
    geometry: &Geometry,
    groups: &TransducerGroups<K>,
    compute: F,
    phases: &mut [Vec<Phase>],
    intensities: &mut [Vec<Intensity>],
) -> Result<(), E>
where
    K: Copy + Eq,
    F: FnMut(K, TransducerMask<'_>, &mut [Vec<Phase>], &mut [Vec<Intensity>]) -> Result<(), E>,
{
    let mut scratch_phases = geometry.phase_buffer();
    let mut scratch_intensities = geometry.intensity_buffer();
    group_compute_with(
        groups,
        compute,
        &mut scratch_phases,
        &mut scratch_intensities,
        phases,
        intensities,
    )
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

fn fill_unassigned<K: Copy + Eq, T: Copy>(
    groups: &TransducerGroups<K>,
    null: T,
    dst: &mut [Vec<T>],
) {
    for (dev, slot) in dst.iter_mut().enumerate() {
        for (out, &i) in slot.iter_mut().zip(groups.indices(dev)) {
            if i.is_none() {
                *out = null;
            }
        }
    }
}

fn copy_group<K: Copy + Eq, T: Copy>(
    groups: &TransducerGroups<K>,
    index: usize,
    scratch: &[Vec<T>],
    dst: &mut [Vec<T>],
) {
    for (dev, (slot, source)) in dst.iter_mut().zip(scratch.iter()).enumerate() {
        for ((out, &v), &i) in slot.iter_mut().zip(source).zip(groups.indices(dev)) {
            if i == Some(index) {
                *out = v;
            }
        }
    }
}

pub fn group_compute_with<K, E, F>(
    groups: &TransducerGroups<K>,
    mut compute: F,
    scratch_phases: &mut [Vec<Phase>],
    scratch_intensities: &mut [Vec<Intensity>],
    phases: &mut [Vec<Phase>],
    intensities: &mut [Vec<Intensity>],
) -> Result<(), E>
where
    K: Copy + Eq,
    F: FnMut(K, TransducerMask<'_>, &mut [Vec<Phase>], &mut [Vec<Intensity>]) -> Result<(), E>,
{
    fill_unassigned(groups, Phase::ZERO, phases);
    fill_unassigned(groups, Intensity::MIN, intensities);
    for (index, &key) in groups.keys().iter().enumerate() {
        let Some(mask) = groups.mask(key) else {
            continue;
        };
        for slot in scratch_phases.iter_mut() {
            slot.fill(Phase::ZERO);
        }
        for slot in scratch_intensities.iter_mut() {
            slot.fill(Intensity::MAX);
        }
        compute(key, mask, scratch_phases, scratch_intensities)?;
        assert_same_shape(scratch_phases, phases);
        assert_same_shape(scratch_intensities, intensities);
        copy_group(groups, index, scratch_phases, phases);
        copy_group(groups, index, scratch_intensities, intensities);
    }
    Ok(())
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

    fn sides(geometry: &Geometry) -> TransducerGroups<Side> {
        TransducerGroups::new(geometry, |device, tr| match (device.idx(), tr % 3) {
            (_, 0) => Some(Side::Left),
            (1, 1) => Some(Side::Right),
            _ => None,
        })
    }

    fn assert_sides<T: Copy + PartialEq + core::fmt::Debug>(
        dst: &[Vec<T>],
        left: T,
        right: T,
        null: T,
    ) {
        for (dev, slot) in dst.iter().enumerate() {
            for (tr, &v) in slot.iter().enumerate() {
                let expected = match (dev, tr % 3) {
                    (_, 0) => left,
                    (1, 1) => right,
                    _ => null,
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
            Phase::ZERO,
            &mut dst,
        );

        assert_sides(&dst, Phase(0x10), Phase(0x30), Phase::ZERO);
    }

    #[test]
    fn group_fills_unassigned_transducers_with_the_given_null() {
        let geometry = geometry();
        let mut left = geometry.intensity_buffer();
        fill(Intensity(0x20), &mut left);
        let mut right = geometry.intensity_buffer();
        fill(Intensity(0x40), &mut right);
        let mut dst = geometry.intensity_buffer();

        let groups = sides(&geometry);
        group(
            &geometry,
            &groups,
            |side| match side {
                Side::Left => &left,
                Side::Right => &right,
            },
            Intensity::MIN,
            &mut dst,
        );

        assert_sides(&dst, Intensity(0x20), Intensity(0x40), Intensity::MIN);
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
        let groups = TransducerGroups::new(&geometry, |_, _| Some(()));

        let mut calls = 0;
        group(
            &geometry,
            &groups,
            |()| {
                calls += 1;
                &src
            },
            Phase::ZERO,
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
        let groups = TransducerGroups::new(&geometry, |_, tr| (tr % 2 == 0).then_some(Side::Left));

        let mut expected = geometry.phase_buffer();
        group(&geometry, &groups, |_| &src, Phase::ZERO, &mut expected);

        let mut dst = vec![Phase(0xFF); Autd3::NUM_TRANSDUCERS];
        group_device(&geometry[1], &groups, |_| &src, Phase::ZERO, &mut dst);
        assert_eq!(dst, expected[1]);
    }

    #[test]
    fn group_compute_passes_the_mask_of_each_key_and_copies_its_transducers() {
        let geometry = geometry();
        let groups = sides(&geometry);
        let mut phases = geometry.phase_buffer();
        fill(Phase(0xFF), &mut phases);
        let mut intensities = geometry.intensity_buffer();

        let mut seen = Vec::new();
        let result: Result<(), ()> = group_compute(
            &geometry,
            &groups,
            |side, mask, phases, intensities| {
                seen.push(side);
                for (dev, slot) in phases.iter().enumerate() {
                    for tr in 0..slot.len() {
                        assert_eq!(mask.is_enabled(dev, tr), groups.key(dev, tr) == Some(side));
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
        assert_sides(&phases, Phase(0x10), Phase(0x30), Phase::ZERO);
        assert_sides(
            &intensities,
            Intensity(0x20),
            Intensity(0x40),
            Intensity::MIN,
        );
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
    fn group_compute_with_hands_a_fresh_pattern_buffer_to_each_key() {
        let geometry = geometry();
        let groups = sides(&geometry);
        let mut scratch_phases = geometry.phase_buffer();
        fill(Phase(0xAA), &mut scratch_phases);
        let mut scratch_intensities = geometry.intensity_buffer();
        fill(Intensity(0xBB), &mut scratch_intensities);
        let mut phases = geometry.phase_buffer();
        fill(Phase(0xFF), &mut phases);
        let mut intensities = geometry.intensity_buffer();

        let result: Result<(), ()> = group_compute_with(
            &groups,
            |side, _, phases, intensities| {
                if side == Side::Left {
                    fill(Phase(0x10), phases);
                    fill(Intensity(0x20), intensities);
                }
                Ok(())
            },
            &mut scratch_phases,
            &mut scratch_intensities,
            &mut phases,
            &mut intensities,
        );

        assert_eq!(result, Ok(()));
        assert_sides(&phases, Phase(0x10), Phase::ZERO, Phase::ZERO);
        assert_sides(
            &intensities,
            Intensity(0x20),
            Intensity::MAX,
            Intensity::MIN,
        );
    }
}
