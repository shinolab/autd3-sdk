use std::sync::Arc;

use crate::legacy::wire::TxFrame;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyFrame {
    frames: Arc<Vec<TxFrame>>,
    start: usize,
    num_devices: usize,
}

impl LegacyFrame {
    pub(crate) fn frames(&self) -> &[TxFrame] {
        &self.frames[self.start..self.start + self.num_devices]
    }

    #[must_use]
    pub const fn num_devices(&self) -> usize {
        self.num_devices
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LegacyFrames {
    frames: Arc<Vec<TxFrame>>,
    num_devices: usize,
}

impl LegacyFrames {
    #[must_use]
    pub fn len(&self) -> usize {
        self.frames.len().checked_div(self.num_devices).unwrap_or(0)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    #[must_use]
    pub fn frame(&self, index: usize) -> Option<LegacyFrame> {
        if self.num_devices == 0 || index >= self.len() {
            return None;
        }
        Some(LegacyFrame {
            frames: Arc::clone(&self.frames),
            start: index * self.num_devices,
            num_devices: self.num_devices,
        })
    }

    #[must_use]
    pub fn iter(&self) -> LegacyFrameIter<'_> {
        LegacyFrameIter {
            frames: self,
            index: 0,
        }
    }

    pub(crate) fn clear(&mut self) {
        match Arc::get_mut(&mut self.frames) {
            Some(frames) => frames.clear(),
            None => self.frames = Arc::default(),
        }
        self.num_devices = 0;
    }

    pub(crate) fn reserve_round(&mut self, num_devices: usize) -> &mut [TxFrame] {
        debug_assert!(self.num_devices == 0 || self.num_devices == num_devices);
        self.num_devices = num_devices;
        let frames = Arc::make_mut(&mut self.frames);
        let start = frames.len();
        frames.resize(start + num_devices, TxFrame::new());
        &mut frames[start..]
    }
}

pub struct LegacyFrameIter<'a> {
    frames: &'a LegacyFrames,
    index: usize,
}

impl Iterator for LegacyFrameIter<'_> {
    type Item = LegacyFrame;

    fn next(&mut self) -> Option<LegacyFrame> {
        let frame = self.frames.frame(self.index)?;
        self.index += 1;
        Some(frame)
    }
}

impl<'a> IntoIterator for &'a LegacyFrames {
    type Item = LegacyFrame;
    type IntoIter = LegacyFrameIter<'a>;

    fn into_iter(self) -> LegacyFrameIter<'a> {
        self.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frames(rounds: usize, num_devices: usize) -> LegacyFrames {
        let mut out = LegacyFrames::default();
        for round in 0..rounds {
            for (device, frame) in out.reserve_round(num_devices).iter_mut().enumerate() {
                frame.payload[0] = u8::try_from(round * 16 + device).unwrap();
            }
        }
        out
    }

    #[test]
    fn empty_frames_have_no_rounds() {
        let out = LegacyFrames::default();
        assert_eq!(out.len(), 0);
        assert!(out.is_empty());
        assert!(out.frame(0).is_none());
        assert_eq!(out.iter().count(), 0);
    }

    #[test]
    fn rounds_are_sliced_per_device() {
        let out = frames(3, 2);
        assert_eq!(out.len(), 3);
        assert!(!out.is_empty());
        for round in 0..3 {
            let frame = out.frame(round).expect("round exists");
            assert_eq!(frame.num_devices(), 2);
            for (device, tx) in frame.frames().iter().enumerate() {
                assert_eq!(tx.payload[0], u8::try_from(round * 16 + device).unwrap());
            }
        }
        assert!(out.frame(3).is_none());
        assert_eq!(out.iter().count(), 3);
        assert_eq!((&out).into_iter().count(), 3);
    }

    #[test]
    fn clear_resets_the_device_count() {
        let mut out = frames(2, 4);
        out.clear();
        assert_eq!(out.len(), 0);
        assert!(out.is_empty());
    }

    #[test]
    fn an_outstanding_frame_keeps_its_own_view_after_a_rebuild() {
        let mut out = frames(2, 2);
        let held = out.frame(1).unwrap();
        out.clear();
        for (device, frame) in out.reserve_round(2).iter_mut().enumerate() {
            frame.payload[0] = u8::try_from(0xF0 + device).unwrap();
        }

        assert_eq!(held.num_devices(), 2);
        assert_eq!(held.frames()[0].payload[0], 16);
        assert_eq!(held.frames()[1].payload[0], 17);
        assert_eq!(out.frame(0).unwrap().frames()[0].payload[0], 0xF0);
    }
}
