use autd3_rs_core::value::{Intensity, Phase};

pub fn set_intensity_device(intensity: Intensity, dst: &mut [Intensity]) {
    dst.fill(intensity);
}

pub fn set_intensity(intensity: Intensity, dst: &mut [Vec<Intensity>]) {
    for slot in &mut *dst {
        set_intensity_device(intensity, slot);
    }
}

pub fn set_phase_device(phase: Phase, dst: &mut [Phase]) {
    dst.fill(phase);
}

pub fn set_phase(phase: Phase, dst: &mut [Vec<Phase>]) {
    for slot in &mut *dst {
        set_phase_device(phase, slot);
    }
}

pub fn add_phase_device(phase: Phase, dst: &mut [Phase]) {
    for p in dst.iter_mut() {
        *p += phase;
    }
}

pub fn add_phase(phase: Phase, dst: &mut [Vec<Phase>]) {
    for slot in &mut *dst {
        add_phase_device(phase, slot);
    }
}

#[cfg(test)]
mod tests {
    use autd3_rs_core::geometry::{Autd3, Geometry, Point3, UnitQuaternion};

    use super::*;

    fn geometry() -> Geometry {
        Geometry::new(vec![
            Autd3::default(),
            Autd3::new(Point3::new(200.0, 0.0, 0.0), UnitQuaternion::identity()),
        ])
    }

    fn ramp(geometry: &Geometry) -> Vec<Vec<Phase>> {
        let mut buf = geometry.phase_buffer();
        for (dev, slot) in buf.iter_mut().enumerate() {
            for (tr, p) in slot.iter_mut().enumerate() {
                *p = Phase(u8::try_from((tr + dev) % 256).unwrap());
            }
        }
        buf
    }

    #[test]
    fn set_intensity_fills_every_transducer() {
        let geometry = geometry();
        let mut buf = geometry.intensity_buffer();
        set_intensity(Intensity(0x80), &mut buf);
        for slot in &buf {
            assert_eq!(slot.len(), Autd3::NUM_TRANSDUCERS);
            assert!(slot.iter().all(|&i| i == Intensity(0x80)));
        }
    }

    #[test]
    fn set_phase_fills_every_transducer() {
        let geometry = geometry();
        let mut buf = ramp(&geometry);
        set_phase(Phase(0x40), &mut buf);
        for slot in &buf {
            assert_eq!(slot.len(), Autd3::NUM_TRANSDUCERS);
            assert!(slot.iter().all(|&p| p == Phase(0x40)));
        }
    }

    #[test]
    fn add_phase_wraps() {
        let geometry = geometry();
        let original = ramp(&geometry);
        let mut buf = original.clone();
        add_phase(Phase(0xF0), &mut buf);
        for (slot, orig) in buf.iter().zip(&original) {
            for (p, o) in slot.iter().zip(orig) {
                assert_eq!(p.0, o.0.wrapping_add(0xF0));
            }
        }
    }

    #[test]
    fn device_level_matches_geometry_level() {
        let geometry = geometry();
        let original = ramp(&geometry);

        let mut whole = original.clone();
        set_phase(Phase(0x22), &mut whole);
        add_phase(Phase(0x33), &mut whole);

        let mut per_device = original;
        for slot in &mut per_device {
            set_phase_device(Phase(0x22), slot);
            add_phase_device(Phase(0x33), slot);
        }
        assert_eq!(whole, per_device);

        let mut whole = geometry.intensity_buffer();
        set_intensity(Intensity(0x55), &mut whole);
        let mut per_device = geometry.intensity_buffer();
        for slot in &mut per_device {
            set_intensity_device(Intensity(0x55), slot);
        }
        assert_eq!(whole, per_device);
    }
}
