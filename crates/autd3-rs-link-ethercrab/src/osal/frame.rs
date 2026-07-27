const ETHERNET_HEADER_BYTES: usize = 14;
const SOURCE_MAC_OFFSET: usize = 6;
const LOCALLY_ADMINISTERED_BIT: u8 = 0x02;

#[cfg(any(target_os = "windows", target_os = "macos"))]
pub(crate) const MAX_FRAME_BYTES: usize = 1536;

pub(crate) fn normalize_source_mac<'a>(frame: &'a [u8], scratch: &'a mut [u8]) -> &'a [u8] {
    if frame.len() < ETHERNET_HEADER_BYTES || frame.len() > scratch.len() {
        return frame;
    }
    let normalized = &mut scratch[..frame.len()];
    normalized.copy_from_slice(frame);
    normalized[SOURCE_MAC_OFFSET] |= LOCALLY_ADMINISTERED_BIT;
    normalized
}

#[cfg(target_os = "windows")]
pub(crate) struct EchoFilter {
    slots: Vec<Vec<u8>>,
    next: usize,
}

#[cfg(target_os = "windows")]
impl EchoFilter {
    const SLOTS: usize = 8;

    pub(crate) fn new() -> Self {
        Self {
            slots: (0..Self::SLOTS)
                .map(|_| Vec::with_capacity(MAX_FRAME_BYTES))
                .collect(),
            next: 0,
        }
    }

    pub(crate) fn record(&mut self, frame: &[u8]) {
        let slot = &mut self.slots[self.next];
        slot.clear();
        slot.extend_from_slice(frame);
        self.next = (self.next + 1) % Self::SLOTS;
    }

    pub(crate) fn take(&mut self, frame: &[u8]) -> bool {
        let Some(slot) = self
            .slots
            .iter_mut()
            .find(|slot| !slot.is_empty() && slot.as_slice() == frame)
        else {
            return false;
        };
        slot.clear();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_source_mac;

    const SCRATCH_BYTES: usize = 1536;

    fn frame(source: [u8; 6]) -> Vec<u8> {
        let mut frame = vec![0u8; 60];
        frame[..6].copy_from_slice(&[0xff; 6]);
        frame[6..12].copy_from_slice(&source);
        frame[12..14].copy_from_slice(&0x88a4u16.to_be_bytes());
        frame
    }

    #[test]
    fn an_untagged_reply_gains_the_locally_administered_bit() {
        let reply = frame([0x10; 6]);
        let mut scratch = vec![0u8; SCRATCH_BYTES];
        let normalized = normalize_source_mac(&reply, &mut scratch);
        assert_eq!(&normalized[6..12], &[0x12, 0x10, 0x10, 0x10, 0x10, 0x10]);
        assert_eq!(&normalized[..6], &[0xff; 6]);
        assert_eq!(&normalized[12..], &reply[12..]);
    }

    #[test]
    fn a_reply_the_subdevice_already_tagged_is_left_alone() {
        let reply = frame([0x12, 0x10, 0x10, 0x10, 0x10, 0x10]);
        let mut scratch = vec![0u8; SCRATCH_BYTES];
        assert_eq!(normalize_source_mac(&reply, &mut scratch), reply.as_slice());
    }

    #[test]
    fn a_frame_too_short_to_hold_a_header_is_passed_through() {
        let runt = [0xffu8; 8];
        let mut scratch = vec![0u8; SCRATCH_BYTES];
        assert_eq!(normalize_source_mac(&runt, &mut scratch), &runt[..]);
    }

    #[test]
    fn a_frame_larger_than_the_scratch_buffer_is_passed_through() {
        let jumbo = vec![0u8; 64];
        let mut scratch = vec![0u8; 32];
        assert_eq!(normalize_source_mac(&jumbo, &mut scratch), jumbo.as_slice());
    }
}

#[cfg(all(test, target_os = "windows"))]
mod echo_tests {
    use super::EchoFilter;

    #[test]
    fn a_recorded_frame_is_rejected_once_and_only_once() {
        let mut filter = EchoFilter::new();
        filter.record(&[1, 2, 3]);
        assert!(filter.take(&[1, 2, 3]));
        assert!(!filter.take(&[1, 2, 3]));
    }

    #[test]
    fn a_frame_that_was_never_sent_is_kept() {
        let mut filter = EchoFilter::new();
        filter.record(&[1, 2, 3]);
        assert!(!filter.take(&[1, 2, 4]));
    }

    #[test]
    fn identical_frames_are_rejected_as_many_times_as_they_were_sent() {
        let mut filter = EchoFilter::new();
        filter.record(&[1, 2, 3]);
        filter.record(&[1, 2, 3]);
        assert!(filter.take(&[1, 2, 3]));
        assert!(filter.take(&[1, 2, 3]));
        assert!(!filter.take(&[1, 2, 3]));
    }

    #[test]
    fn an_empty_slot_never_matches_an_empty_frame() {
        let mut filter = EchoFilter::new();
        assert!(!filter.take(&[]));
    }

    #[test]
    fn the_oldest_record_is_evicted_once_the_ring_wraps() {
        let mut filter = EchoFilter::new();
        for i in 0..=u8::try_from(EchoFilter::SLOTS).unwrap() {
            filter.record(&[i]);
        }
        assert!(!filter.take(&[0]));
        assert!(filter.take(&[1]));
    }
}
