use super::segment::Segment;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FpgaState(pub u8);

impl FpgaState {
    const THERMAL_ASSERT_BIT: u8 = 1 << 0;
    const CURRENT_MOD_SEGMENT_BIT: u8 = 1 << 1;
    const CURRENT_STM_SEGMENT_BIT: u8 = 1 << 2;
    const IS_GAIN_MODE_BIT: u8 = 1 << 3;
    pub const READS_FPGA_STATE_ENABLED: u8 = 1 << 7;

    #[must_use]
    pub const fn is_valid(self) -> bool {
        (self.0 & Self::READS_FPGA_STATE_ENABLED) != 0
    }

    #[must_use]
    pub const fn is_thermal_assert(self) -> bool {
        (self.0 & Self::THERMAL_ASSERT_BIT) != 0
    }

    #[must_use]
    pub const fn current_mod_segment(self) -> Segment {
        if (self.0 & Self::CURRENT_MOD_SEGMENT_BIT) != 0 {
            Segment::S1
        } else {
            Segment::S0
        }
    }

    #[must_use]
    pub const fn is_gain_mode(self) -> bool {
        (self.0 & Self::IS_GAIN_MODE_BIT) != 0
    }

    #[must_use]
    pub const fn is_stm_mode(self) -> bool {
        !self.is_gain_mode()
    }

    #[must_use]
    pub const fn current_segment(self) -> Segment {
        if (self.0 & Self::CURRENT_STM_SEGMENT_BIT) != 0 {
            Segment::S1
        } else {
            Segment::S0
        }
    }

    #[must_use]
    pub const fn current_stm_segment(self) -> Option<Segment> {
        if self.is_stm_mode() {
            Some(self.current_segment())
        } else {
            None
        }
    }

    #[must_use]
    pub const fn current_gain_segment(self) -> Option<Segment> {
        if self.is_gain_mode() {
            Some(self.current_segment())
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validity_follows_bit7() {
        assert!(!FpgaState(0x00).is_valid());
        assert!(FpgaState(0x80).is_valid());
    }

    #[test]
    fn thermal_and_segment_bits() {
        assert!(FpgaState(0x01).is_thermal_assert());
        assert!(!FpgaState(0x00).is_thermal_assert());
        assert_eq!(FpgaState(0x00).current_mod_segment(), Segment::S0);
        assert_eq!(FpgaState(0x02).current_mod_segment(), Segment::S1);
    }

    #[test]
    fn stm_and_gain_segments_are_mutually_exclusive() {
        let stm = FpgaState(0x04);
        assert!(stm.is_stm_mode());
        assert_eq!(stm.current_stm_segment(), Some(Segment::S1));
        assert_eq!(stm.current_gain_segment(), None);

        let gain = FpgaState(0x08);
        assert!(gain.is_gain_mode());
        assert_eq!(gain.current_gain_segment(), Some(Segment::S0));
        assert_eq!(gain.current_stm_segment(), None);
    }
}
