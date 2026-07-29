use autd3_rs_core::geometry::Device;
use autd3_rs_core::value::{Phase, PulseWidth};
use zerocopy::{Immutable, IntoBytes};

use super::LegacyOperation;
use crate::legacy::error::{LegacyError, PayloadError};
use crate::legacy::wire::params::{
    GPIO_IN_FLAG_0, GPIO_IN_FLAG_1, GPIO_IN_FLAG_2, GPIO_IN_FLAG_3, PWE_TABLE_SIZE,
};
use crate::legacy::wire::{GpioOut, Segment, Tag};

#[repr(C)]
#[derive(Clone, Copy, IntoBytes, Immutable)]
struct TagPair {
    tag: u8,
    value: u8,
}

#[repr(C)]
#[derive(Clone, Copy, IntoBytes, Immutable)]
struct GpioOutHead {
    tag: u8,
    pad: [u8; 7],
}

fn slot_for<'a, T>(items: &'a [Vec<T>], device: &Device) -> Result<&'a [T], PayloadError> {
    let slot = items
        .get(device.idx())
        .ok_or(PayloadError::EmissionDeviceCountMismatch {
            expected: device.idx() + 1,
            got: items.len(),
        })?;
    if slot.len() != device.num_transducers() {
        return Err(PayloadError::EmissionTransducerCountMismatch {
            device: device.idx(),
            expected: device.num_transducers(),
            got: slot.len(),
        });
    }
    Ok(slot)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetOutputMask<'a> {
    masks: &'a [Vec<bool>],
    segment: Segment,
    done: bool,
}

impl<'a> SetOutputMask<'a> {
    #[must_use]
    pub const fn new(masks: &'a [Vec<bool>], segment: Segment) -> Self {
        Self {
            masks,
            segment,
            done: false,
        }
    }
}

impl LegacyOperation for SetOutputMask<'_> {
    fn required_size(&self, device: &Device) -> usize {
        size_of::<TagPair>() + device.num_transducers().div_ceil(8)
    }

    fn pack(&mut self, device: &Device, tx: &mut [u8]) -> Result<usize, LegacyError> {
        let mask = slot_for(self.masks, device)?;
        let head = TagPair {
            tag: Tag::OutputMask.as_u8(),
            value: self.segment.as_u8(),
        };
        tx[..size_of::<TagPair>()].copy_from_slice(head.as_bytes());
        for (dst, chunk) in tx[size_of::<TagPair>()..].iter_mut().zip(mask.chunks(8)) {
            *dst = chunk.iter().enumerate().fold(
                0u8,
                |acc, (bit, &on)| if on { acc | (1 << bit) } else { acc },
            );
        }
        self.done = true;
        Ok(self.required_size(device))
    }

    fn is_done(&self) -> bool {
        self.done
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetPhaseCorrection<'a> {
    phases: &'a [Vec<Phase>],
    done: bool,
}

impl<'a> SetPhaseCorrection<'a> {
    #[must_use]
    pub const fn new(phases: &'a [Vec<Phase>]) -> Self {
        Self {
            phases,
            done: false,
        }
    }
}

impl LegacyOperation for SetPhaseCorrection<'_> {
    fn required_size(&self, device: &Device) -> usize {
        size_of::<TagPair>() + device.num_transducers().next_multiple_of(2)
    }

    fn pack(&mut self, device: &Device, tx: &mut [u8]) -> Result<usize, LegacyError> {
        let phases = slot_for(self.phases, device)?;
        let head = TagPair {
            tag: Tag::PhaseCorrection.as_u8(),
            value: 0,
        };
        tx[..size_of::<TagPair>()].copy_from_slice(head.as_bytes());
        for (dst, phase) in tx[size_of::<TagPair>()..].iter_mut().zip(phases) {
            *dst = phase.0;
        }
        self.done = true;
        Ok(self.required_size(device))
    }

    fn is_done(&self) -> bool {
        self.done
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SetPulseWidthTable<'a> {
    table: &'a [PulseWidth; PWE_TABLE_SIZE],
    done: bool,
}

impl<'a> SetPulseWidthTable<'a> {
    #[must_use]
    pub const fn new(table: &'a [PulseWidth; PWE_TABLE_SIZE]) -> Self {
        Self { table, done: false }
    }
}

impl LegacyOperation for SetPulseWidthTable<'_> {
    fn required_size(&self, _device: &Device) -> usize {
        size_of::<TagPair>() + PWE_TABLE_SIZE * size_of::<u16>()
    }

    fn pack(&mut self, device: &Device, tx: &mut [u8]) -> Result<usize, LegacyError> {
        let head = TagPair {
            tag: Tag::ConfigPulseWidthEncoder.as_u8(),
            value: 0,
        };
        tx[..size_of::<TagPair>()].copy_from_slice(head.as_bytes());
        for (dst, entry) in tx[size_of::<TagPair>()..]
            .chunks_exact_mut(size_of::<u16>())
            .zip(self.table.iter())
        {
            dst.copy_from_slice(&entry.pulse_width()?.to_le_bytes());
        }
        self.done = true;
        Ok(self.required_size(device))
    }

    fn is_done(&self) -> bool {
        self.done
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetGpioOut {
    outputs: [GpioOut; 4],
    done: bool,
}

impl SetGpioOut {
    #[must_use]
    pub const fn new(outputs: [GpioOut; 4]) -> Self {
        Self {
            outputs,
            done: false,
        }
    }
}

impl LegacyOperation for SetGpioOut {
    fn required_size(&self, _device: &Device) -> usize {
        size_of::<GpioOutHead>() + 4 * size_of::<u64>()
    }

    fn pack(&mut self, device: &Device, tx: &mut [u8]) -> Result<usize, LegacyError> {
        let head = GpioOutHead {
            tag: Tag::FpgaGpioOut.as_u8(),
            pad: [0; 7],
        };
        tx[..size_of::<GpioOutHead>()].copy_from_slice(head.as_bytes());
        for (dst, output) in tx[size_of::<GpioOutHead>()..]
            .chunks_exact_mut(size_of::<u64>())
            .zip(self.outputs)
        {
            dst.copy_from_slice(&output.encode().to_le_bytes());
        }
        self.done = true;
        Ok(self.required_size(device))
    }

    fn is_done(&self) -> bool {
        self.done
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EmulateGpioIn {
    values: [bool; 4],
    done: bool,
}

impl EmulateGpioIn {
    #[must_use]
    pub const fn new(values: [bool; 4]) -> Self {
        Self {
            values,
            done: false,
        }
    }
}

impl LegacyOperation for EmulateGpioIn {
    fn required_size(&self, _device: &Device) -> usize {
        size_of::<TagPair>()
    }

    fn pack(&mut self, _device: &Device, tx: &mut [u8]) -> Result<usize, LegacyError> {
        let mut flag = 0u8;
        for (bit, on) in [
            GPIO_IN_FLAG_0,
            GPIO_IN_FLAG_1,
            GPIO_IN_FLAG_2,
            GPIO_IN_FLAG_3,
        ]
        .into_iter()
        .zip(self.values)
        {
            if on {
                flag |= bit;
            }
        }
        let msg = TagPair {
            tag: Tag::EmulateGpioIn.as_u8(),
            value: flag,
        };
        tx[..size_of::<TagPair>()].copy_from_slice(msg.as_bytes());
        self.done = true;
        Ok(size_of::<TagPair>())
    }

    fn is_done(&self) -> bool {
        self.done
    }
}

#[cfg(test)]
mod tests {
    use autd3_rs_core::geometry::{Autd3, Geometry};
    use autd3_rs_core::value::DcSysTime;

    use super::*;
    use crate::legacy::wire::PAYLOAD_BYTES;

    fn geometry(n: usize) -> Geometry {
        Geometry::new((0..n).map(|_| Autd3::default()).collect())
    }

    #[test]
    fn output_mask_is_bit_packed_lsb_first() {
        let geo = geometry(1);
        let n = geo[0].num_transducers();
        let mut mask = vec![false; n];
        mask[0] = true;
        mask[3] = true;
        mask[8] = true;
        mask[n - 1] = true;
        let masks = vec![mask];

        let mut op = SetOutputMask::new(&masks, Segment::S1);
        assert_eq!(op.required_size(&geo[0]), 2 + n.div_ceil(8));
        let mut tx = vec![0u8; PAYLOAD_BYTES];
        assert_eq!(op.pack(&geo[0], &mut tx).unwrap(), 2 + n.div_ceil(8));

        assert_eq!(tx[0], Tag::OutputMask.as_u8());
        assert_eq!(tx[1], Segment::S1.as_u8());
        assert_eq!(tx[2], 0b0000_1001, "transducers 0 and 3");
        assert_eq!(tx[3], 0b0000_0001, "transducer 8");
        assert_eq!(tx[2 + (n - 1) / 8], 1 << ((n - 1) % 8));
    }

    #[test]
    fn phase_correction_is_one_byte_per_transducer() {
        let geo = geometry(1);
        let n = geo[0].num_transducers();
        let phases = vec![
            (0..n)
                .map(|i| Phase(u8::try_from(i % 256).unwrap()))
                .collect::<Vec<_>>(),
        ];

        let mut op = SetPhaseCorrection::new(&phases);
        assert_eq!(op.required_size(&geo[0]), 2 + n.next_multiple_of(2));
        let mut tx = vec![0u8; PAYLOAD_BYTES];
        op.pack(&geo[0], &mut tx).unwrap();

        assert_eq!(tx[0], Tag::PhaseCorrection.as_u8());
        assert_eq!(tx[1], 0);
        for (i, phase) in phases[0].iter().enumerate() {
            assert_eq!(tx[2 + i], phase.0);
        }
    }

    #[test]
    fn pulse_width_table_is_256_little_endian_words() {
        let geo = geometry(1);
        let table: [PulseWidth; PWE_TABLE_SIZE] =
            core::array::from_fn(|i| PulseWidth::new(u16::try_from(i).unwrap()));

        let mut op = SetPulseWidthTable::new(&table);
        assert_eq!(op.required_size(&geo[0]), 2 + 512);
        let mut tx = vec![0u8; PAYLOAD_BYTES];
        assert_eq!(op.pack(&geo[0], &mut tx).unwrap(), 2 + 512);

        assert_eq!(tx[0], Tag::ConfigPulseWidthEncoder.as_u8());
        for i in 0..PWE_TABLE_SIZE {
            let word = u16::from_le_bytes([tx[2 + i * 2], tx[3 + i * 2]]);
            assert_eq!(word, u16::try_from(i).unwrap());
        }
    }

    #[test]
    fn gpio_out_packs_a_type_tag_into_the_top_byte() {
        let geo = geometry(1);
        let time = DcSysTime::from_nanos(3125 * 512);
        let mut op = SetGpioOut::new([
            GpioOut::Off,
            GpioOut::Thermo,
            GpioOut::PwmOut(42),
            GpioOut::SysTimeEq(time),
        ]);
        assert_eq!(op.required_size(&geo[0]), 8 + 32);

        let mut tx = vec![0u8; PAYLOAD_BYTES];
        assert_eq!(op.pack(&geo[0], &mut tx).unwrap(), 40);
        assert_eq!(tx[0], Tag::FpgaGpioOut.as_u8());
        assert_eq!(&tx[1..8], &[0u8; 7]);

        let word = |i: usize| u64::from_le_bytes(tx[8 + i * 8..16 + i * 8].try_into().unwrap());
        assert_eq!(word(0), 0x00);
        assert_eq!(word(1) >> 56, 0x02);
        assert_eq!(word(2) >> 56, 0xE0);
        assert_eq!(word(2) & 0x00FF_FFFF_FFFF_FFFF, 42);
        assert_eq!(word(3) >> 56, 0x60);
        assert_eq!(word(3) & 0x00FF_FFFF_FFFF_FFFF, (512u64 << 6) >> 9);
    }

    #[test]
    fn emulate_gpio_in_packs_four_bits() {
        let geo = geometry(1);
        for (values, expected) in [
            ([false; 4], 0b0000),
            ([true, false, false, false], 0b0001),
            ([false, true, false, true], 0b1010),
            ([true; 4], 0b1111),
        ] {
            let mut op = EmulateGpioIn::new(values);
            let mut tx = [0u8; 2];
            assert_eq!(op.pack(&geo[0], &mut tx).unwrap(), 2);
            assert_eq!(tx, [Tag::EmulateGpioIn.as_u8(), expected]);
        }
    }
}
