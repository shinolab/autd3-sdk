use autd3_rs_core::geometry::{Device, Geometry, TransducerGroups, TransducerMask};
use autd3_rs_core::value::{Emission, Intensity, Phase};

use crate::set_phase_and_intensity;

fn sources_of<K, S, F>(groups: &TransducerGroups<K>, mut source: F) -> Vec<S>
where
    K: Copy + Eq,
    F: FnMut(K) -> S,
{
    groups.keys().iter().map(|&key| source(key)).collect()
}

fn write_device<K, S>(
    device: &Device,
    groups: &TransducerGroups<K>,
    sources: &[S],
    dst: &mut [Emission],
) where
    K: Copy + Eq,
    S: AsRef<[Vec<Emission>]>,
{
    let dev = device.idx();
    for (tr, (slot, &index)) in dst.iter_mut().zip(groups.indices(dev)).enumerate() {
        *slot = index.map_or(Emission::NULL, |index| sources[index].as_ref()[dev][tr]);
    }
}

pub fn group_device<K, S, F>(
    device: &Device,
    groups: &TransducerGroups<K>,
    source: F,
    dst: &mut [Emission],
) where
    K: Copy + Eq,
    S: AsRef<[Vec<Emission>]>,
    F: FnMut(K) -> S,
{
    let sources = sources_of(groups, source);
    write_device(device, groups, &sources, dst);
}

pub fn group<K, S, F>(
    geometry: &Geometry,
    groups: &TransducerGroups<K>,
    source: F,
    dst: &mut [Vec<Emission>],
) where
    K: Copy + Eq,
    S: AsRef<[Vec<Emission>]>,
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
    compute: F,
    dst: &mut [Vec<Emission>],
) -> Result<(), E>
where
    K: Copy + Eq,
    F: FnMut(K, TransducerMask<'_>, &mut [Vec<Emission>]) -> Result<(), E>,
{
    let mut scratch = geometry.pattern_buffer();
    group_compute_with(groups, compute, &mut scratch, dst)
}

pub fn group_compute_with<K, E, F>(
    groups: &TransducerGroups<K>,
    mut compute: F,
    scratch: &mut [Vec<Emission>],
    dst: &mut [Vec<Emission>],
) -> Result<(), E>
where
    K: Copy + Eq,
    F: FnMut(K, TransducerMask<'_>, &mut [Vec<Emission>]) -> Result<(), E>,
{
    for (dev, slot) in dst.iter_mut().enumerate() {
        for (out, &i) in slot.iter_mut().zip(groups.indices(dev)) {
            if i.is_none() {
                *out = Emission::NULL;
            }
        }
    }
    for (index, &key) in groups.keys().iter().enumerate() {
        let Some(mask) = groups.mask(key) else {
            continue;
        };
        set_phase_and_intensity(Phase::ZERO, Intensity::MAX, scratch);
        compute(key, mask, scratch)?;
        assert!(
            scratch.len() == dst.len()
                && scratch
                    .iter()
                    .zip(dst.iter())
                    .all(|(source, slot)| source.len() == slot.len()),
            "scratch must have the same shape as dst"
        );
        for (dev, (slot, source)) in dst.iter_mut().zip(scratch.iter()).enumerate() {
            for ((out, &e), &i) in slot.iter_mut().zip(source).zip(groups.indices(dev)) {
                if i == Some(index) {
                    *out = e;
                }
            }
        }
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

    fn emission(phase: u8, intensity: u8) -> Emission {
        Emission {
            phase: Phase(phase),
            intensity: Intensity(intensity),
        }
    }

    fn set_emission(e: Emission, dst: &mut [Vec<Emission>]) {
        set_phase_and_intensity(e.phase, e.intensity, dst);
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

    fn assert_sides(dst: &[Vec<Emission>], left: Emission, right: Emission) {
        for (dev, slot) in dst.iter().enumerate() {
            for (tr, &e) in slot.iter().enumerate() {
                let expected = match (dev, tr % 3) {
                    (_, 0) => left,
                    (1, 1) => right,
                    _ => Emission::NULL,
                };
                assert_eq!(e, expected, "dev {dev} tr {tr}");
            }
        }
    }

    #[test]
    fn group_copies_the_source_of_each_key() {
        let geometry = geometry();
        let mut left = geometry.pattern_buffer();
        set_emission(emission(0x10, 0x20), &mut left);
        let mut right = geometry.pattern_buffer();
        set_emission(emission(0x30, 0x40), &mut right);
        let mut dst = geometry.pattern_buffer();
        set_emission(emission(0xFF, 0xFF), &mut dst);

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

        assert_sides(&dst, emission(0x10, 0x20), emission(0x30, 0x40));
    }

    #[test]
    fn group_asks_for_each_source_once_and_reads_the_same_transducer() {
        let geometry = geometry();
        let mut src = geometry.pattern_buffer();
        for (dev, slot) in src.iter_mut().enumerate() {
            for (tr, e) in slot.iter_mut().enumerate() {
                *e = emission(u8::try_from(tr % 256).unwrap(), u8::try_from(dev).unwrap());
            }
        }
        let mut dst = geometry.pattern_buffer();
        let groups = TransducerGroups::new(&geometry, |_, _| Some(()));

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
        let mut src = geometry.pattern_buffer();
        set_emission(emission(0x55, 0x66), &mut src);
        let groups = TransducerGroups::new(&geometry, |_, tr| (tr % 2 == 0).then_some(Side::Left));

        let mut expected = geometry.pattern_buffer();
        group(&geometry, &groups, |_| &src, &mut expected);

        let mut dst = vec![emission(0xFF, 0xFF); Autd3::NUM_TRANSDUCERS];
        group_device(&geometry[1], &groups, |_| &src, &mut dst);
        assert_eq!(dst, expected[1]);
    }

    #[test]
    fn group_compute_passes_the_mask_of_each_key_and_copies_its_transducers() {
        let geometry = geometry();
        let groups = sides(&geometry);
        let mut dst = geometry.pattern_buffer();
        set_emission(emission(0xFF, 0xFF), &mut dst);

        let mut seen = Vec::new();
        let result: Result<(), ()> = group_compute(
            &geometry,
            &groups,
            |side, mask, buffer| {
                seen.push(side);
                for (dev, slot) in buffer.iter().enumerate() {
                    for tr in 0..slot.len() {
                        assert_eq!(mask.is_enabled(dev, tr), groups.key(dev, tr) == Some(side));
                    }
                }
                let e = match side {
                    Side::Left => emission(0x10, 0x20),
                    Side::Right => emission(0x30, 0x40),
                };
                set_emission(e, buffer);
                Ok(())
            },
            &mut dst,
        );

        assert_eq!(result, Ok(()));
        assert_eq!(seen, [Side::Left, Side::Right]);
        assert_sides(&dst, emission(0x10, 0x20), emission(0x30, 0x40));
    }

    #[test]
    fn group_compute_stops_at_the_first_error() {
        let geometry = geometry();
        let groups = sides(&geometry);
        let mut dst = geometry.pattern_buffer();

        let mut calls = 0;
        let result = group_compute(
            &geometry,
            &groups,
            |side, _, _| {
                calls += 1;
                if side == Side::Left {
                    Err("left failed")
                } else {
                    Ok(())
                }
            },
            &mut dst,
        );

        assert_eq!(result, Err("left failed"));
        assert_eq!(calls, 1);
    }

    #[test]
    fn group_compute_with_hands_a_fresh_pattern_buffer_to_each_key() {
        let geometry = geometry();
        let groups = sides(&geometry);
        let mut scratch = geometry.pattern_buffer();
        set_emission(emission(0xAA, 0xBB), &mut scratch);
        let mut dst = geometry.pattern_buffer();
        set_emission(emission(0xFF, 0xFF), &mut dst);

        let result: Result<(), ()> = group_compute_with(
            &groups,
            |side, _, buffer| {
                if side == Side::Left {
                    set_emission(emission(0x10, 0x20), buffer);
                }
                Ok(())
            },
            &mut scratch,
            &mut dst,
        );

        assert_eq!(result, Ok(()));
        assert_sides(&dst, emission(0x10, 0x20), emission(0x00, 0xFF));
    }
}
