use autd3_rs_core::value::{Emission, Intensity, Phase};

pub fn set_intensity_device(intensity: Intensity, dst: &mut [Emission]) {
    for e in dst.iter_mut() {
        e.intensity = intensity;
    }
}

pub fn set_intensity(intensity: Intensity, dst: &mut [Vec<Emission>]) {
    for slot in &mut *dst {
        set_intensity_device(intensity, slot);
    }
}

pub fn set_phase_device(phase: Phase, dst: &mut [Emission]) {
    for e in dst.iter_mut() {
        e.phase = phase;
    }
}

pub fn set_phase(phase: Phase, dst: &mut [Vec<Emission>]) {
    for slot in &mut *dst {
        set_phase_device(phase, slot);
    }
}

pub fn set_phase_and_intensity_device(phase: Phase, intensity: Intensity, dst: &mut [Emission]) {
    dst.fill(Emission { phase, intensity });
}

pub fn set_phase_and_intensity(phase: Phase, intensity: Intensity, dst: &mut [Vec<Emission>]) {
    for slot in &mut *dst {
        set_phase_and_intensity_device(phase, intensity, slot);
    }
}

pub fn add_phase_device(phase: Phase, dst: &mut [Emission]) {
    for e in dst.iter_mut() {
        e.phase += phase;
    }
}

pub fn add_phase(phase: Phase, dst: &mut [Vec<Emission>]) {
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

    fn ramp(geometry: &Geometry) -> Vec<Vec<Emission>> {
        let mut buf = geometry.pattern_buffer();
        for (dev, slot) in buf.iter_mut().enumerate() {
            for (tr, e) in slot.iter_mut().enumerate() {
                *e = Emission {
                    phase: Phase(u8::try_from((tr + dev) % 256).unwrap()),
                    intensity: Intensity(u8::try_from((tr * 3) % 256).unwrap()),
                };
            }
        }
        buf
    }

    #[test]
    fn set_intensity_keeps_phase() {
        let geometry = geometry();
        let original = ramp(&geometry);
        let mut buf = original.clone();
        set_intensity(Intensity(0x80), &mut buf);
        for (slot, orig) in buf.iter().zip(&original) {
            for (e, o) in slot.iter().zip(orig) {
                assert_eq!(e.phase, o.phase);
                assert_eq!(e.intensity, Intensity(0x80));
            }
        }
    }

    #[test]
    fn set_phase_keeps_intensity() {
        let geometry = geometry();
        let original = ramp(&geometry);
        let mut buf = original.clone();
        set_phase(Phase(0x40), &mut buf);
        for (slot, orig) in buf.iter().zip(&original) {
            for (e, o) in slot.iter().zip(orig) {
                assert_eq!(e.phase, Phase(0x40));
                assert_eq!(e.intensity, o.intensity);
            }
        }
    }

    #[test]
    fn set_phase_and_intensity_fills_every_transducer() {
        let geometry = geometry();
        let mut buf = ramp(&geometry);
        set_phase_and_intensity(Phase(0x40), Intensity(0x80), &mut buf);
        for slot in &buf {
            assert_eq!(slot.len(), Autd3::NUM_TRANSDUCERS);
            for &e in slot {
                assert_eq!(
                    e,
                    Emission {
                        phase: Phase(0x40),
                        intensity: Intensity(0x80),
                    }
                );
            }
        }
    }

    #[test]
    fn add_phase_wraps_and_keeps_intensity() {
        let geometry = geometry();
        let original = ramp(&geometry);
        let mut buf = original.clone();
        add_phase(Phase(0xF0), &mut buf);
        for (slot, orig) in buf.iter().zip(&original) {
            for (e, o) in slot.iter().zip(orig) {
                assert_eq!(e.phase.0, o.phase.0.wrapping_add(0xF0));
                assert_eq!(e.intensity, o.intensity);
            }
        }
    }

    #[test]
    fn device_level_matches_geometry_level() {
        let geometry = geometry();
        let original = ramp(&geometry);

        let mut whole = original.clone();
        set_intensity(Intensity(0x11), &mut whole);
        set_phase(Phase(0x22), &mut whole);
        add_phase(Phase(0x33), &mut whole);

        let mut per_device = original;
        for slot in &mut per_device {
            set_intensity_device(Intensity(0x11), slot);
            set_phase_device(Phase(0x22), slot);
            add_phase_device(Phase(0x33), slot);
        }
        assert_eq!(whole, per_device);

        let mut whole = geometry.pattern_buffer();
        set_phase_and_intensity(Phase(0x44), Intensity(0x55), &mut whole);
        let mut per_device = geometry.pattern_buffer();
        for slot in &mut per_device {
            set_phase_and_intensity_device(Phase(0x44), Intensity(0x55), slot);
        }
        assert_eq!(whole, per_device);
    }
}
