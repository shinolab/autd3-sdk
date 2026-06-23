#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::unreadable_literal
)]

use autd3_rs_core::value::{Emission, Intensity, Phase};

use super::FpgaEmulator;
use crate::ffi;

pub(super) static SIN_TABLE: &[u8; 256] = include_bytes!("sin.dat");
pub(super) static ATAN_TABLE: &[u8; 16384] = include_bytes!("atan.dat");

#[rustfmt::skip]
pub(super) static TR_POS: [u32; 252] = [
    0x00000000,0x01960000,0x032d0000,0x04c30000,0x065a0000,0x07f00000,0x09860000,0x0b1d0000,
    0x0cb30000,0x0e4a0000,0x0fe00000,0x11760000,0x130d0000,0x14a30000,0x163a0000,0x17d00000,
    0x19660000,0x1afd0000,0x00000196,0x04c30196,0x065a0196,0x07f00196,0x09860196,0x0b1d0196,
    0x0cb30196,0x0e4a0196,0x0fe00196,0x11760196,0x130d0196,0x14a30196,0x163a0196,0x17d00196,
    0x1afd0196,0x0000032d,0x0196032d,0x032d032d,0x04c3032d,0x065a032d,0x07f0032d,0x0986032d,
    0x0b1d032d,0x0cb3032d,0x0e4a032d,0x0fe0032d,0x1176032d,0x130d032d,0x14a3032d,0x163a032d,
    0x17d0032d,0x1966032d,0x1afd032d,0x000004c3,0x019604c3,0x032d04c3,0x04c304c3,0x065a04c3,
    0x07f004c3,0x098604c3,0x0b1d04c3,0x0cb304c3,0x0e4a04c3,0x0fe004c3,0x117604c3,0x130d04c3,
    0x14a304c3,0x163a04c3,0x17d004c3,0x196604c3,0x1afd04c3,0x0000065a,0x0196065a,0x032d065a,
    0x04c3065a,0x065a065a,0x07f0065a,0x0986065a,0x0b1d065a,0x0cb3065a,0x0e4a065a,0x0fe0065a,
    0x1176065a,0x130d065a,0x14a3065a,0x163a065a,0x17d0065a,0x1966065a,0x1afd065a,0x000007f0,
    0x019607f0,0x032d07f0,0x04c307f0,0x065a07f0,0x07f007f0,0x098607f0,0x0b1d07f0,0x0cb307f0,
    0x0e4a07f0,0x0fe007f0,0x117607f0,0x130d07f0,0x14a307f0,0x163a07f0,0x17d007f0,0x196607f0,
    0x1afd07f0,0x00000986,0x01960986,0x032d0986,0x04c30986,0x065a0986,0x07f00986,0x09860986,
    0x0b1d0986,0x0cb30986,0x0e4a0986,0x0fe00986,0x11760986,0x130d0986,0x14a30986,0x163a0986,
    0x17d00986,0x19660986,0x1afd0986,0x00000b1d,0x01960b1d,0x032d0b1d,0x04c30b1d,0x065a0b1d,
    0x07f00b1d,0x09860b1d,0x0b1d0b1d,0x0cb30b1d,0x0e4a0b1d,0x0fe00b1d,0x11760b1d,0x130d0b1d,
    0x14a30b1d,0x163a0b1d,0x17d00b1d,0x19660b1d,0x1afd0b1d,0x00000cb3,0x01960cb3,0x032d0cb3,
    0x04c30cb3,0x065a0cb3,0x07f00cb3,0x09860cb3,0x0b1d0cb3,0x0cb30cb3,0x0e4a0cb3,0x0fe00cb3,
    0x11760cb3,0x130d0cb3,0x14a30cb3,0x163a0cb3,0x17d00cb3,0x19660cb3,0x1afd0cb3,0x00000e4a,
    0x01960e4a,0x032d0e4a,0x04c30e4a,0x065a0e4a,0x07f00e4a,0x09860e4a,0x0b1d0e4a,0x0cb30e4a,
    0x0e4a0e4a,0x0fe00e4a,0x11760e4a,0x130d0e4a,0x14a30e4a,0x163a0e4a,0x17d00e4a,0x19660e4a,
    0x1afd0e4a,0x00000fe0,0x01960fe0,0x032d0fe0,0x04c30fe0,0x065a0fe0,0x07f00fe0,0x09860fe0,
    0x0b1d0fe0,0x0cb30fe0,0x0e4a0fe0,0x0fe00fe0,0x11760fe0,0x130d0fe0,0x14a30fe0,0x163a0fe0,
    0x17d00fe0,0x19660fe0,0x1afd0fe0,0x00001176,0x01961176,0x032d1176,0x04c31176,0x065a1176,
    0x07f01176,0x09861176,0x0b1d1176,0x0cb31176,0x0e4a1176,0x0fe01176,0x11761176,0x130d1176,
    0x14a31176,0x163a1176,0x17d01176,0x19661176,0x1afd1176,0x0000130d,0x0196130d,0x032d130d,
    0x04c3130d,0x065a130d,0x07f0130d,0x0986130d,0x0b1d130d,0x0cb3130d,0x0e4a130d,0x0fe0130d,
    0x1176130d,0x130d130d,0x14a3130d,0x163a130d,0x17d0130d,0x1966130d,0x1afd130d,0x000014a3,
    0x019614a3,0x032d14a3,0x04c314a3,0x065a14a3,0x07f014a3,0x098614a3,0x0b1d14a3,0x0cb314a3,
    0x0e4a14a3,0x0fe014a3,0x117614a3,0x130d14a3,0x14a314a3,0x163a14a3,0x17d014a3,0x196614a3,
    0x1afd14a3,0x00000000,0x00000000,0x00000000
];

struct StmFocus(u64);

impl StmFocus {
    fn x(&self) -> i32 {
        sign_extend_18(self.0 & 0x3_FFFF)
    }
    fn y(&self) -> i32 {
        sign_extend_18((self.0 >> 18) & 0x3_FFFF)
    }
    fn z(&self) -> i32 {
        sign_extend_18((self.0 >> 36) & 0x3_FFFF)
    }
    fn intensity(&self) -> u8 {
        ((self.0 >> 54) & 0xFF) as u8
    }
}

fn sign_extend_18(v: u64) -> i32 {
    let v = v as u32;
    if v & 0x2_0000 != 0 {
        (v | 0xFFFC_0000) as i32
    } else {
        v as i32
    }
}

fn read_focus(ram: &[u16], word_base: usize) -> u64 {
    (0..4)
        .map(|k| u64::from(ram[word_base + k]) << (16 * k))
        .sum()
}

impl FpgaEmulator {
    #[must_use]
    pub fn sound_speed(&self, bank: usize) -> u16 {
        self.controller[ffi::ADDR_PATTERN_SOUND_SPEED0 as usize + bank]
    }

    #[must_use]
    pub fn num_foci(&self, bank: usize) -> usize {
        self.controller[ffi::ADDR_PATTERN_NUM_FOCI0 as usize + bank] as usize
    }

    #[must_use]
    pub fn foci_drives_at(&self, bank: usize, idx: usize) -> Vec<Emission> {
        let sound_speed = u32::from(self.sound_speed(bank));
        let num_foci = self.num_foci(bank);
        let ram = &self.em_ram[bank];
        (0..self.num_transducers)
            .map(|i| {
                let tr = TR_POS[i];
                let tr_x = i32::from(((tr >> 16) & 0xFFFF) as i16);
                let tr_y = i32::from((tr & 0xFFFF) as i16);
                let tr_z = 0i32;
                let mut intensity = 0u8;
                let (sin, cos) = (0..num_foci).fold((0u16, 0u16), |acc, f| {
                    let focus = StmFocus(read_focus(ram, 4 * (idx * num_foci + f)));
                    let (x, y, z) = (focus.x(), focus.y(), focus.z());
                    let offset = if f == 0 {
                        intensity = focus.intensity();
                        0
                    } else {
                        focus.intensity()
                    };
                    let d2 =
                        (x - tr_x) * (x - tr_x) + (y - tr_y) * (y - tr_y) + (z - tr_z) * (z - tr_z);
                    let dist = d2.isqrt() as u32;
                    let q = ((dist << 14) / sound_speed) as usize + offset as usize;
                    (
                        acc.0 + u16::from(SIN_TABLE[q % 256]),
                        acc.1 + u16::from(SIN_TABLE[(q + 64) % 256]),
                    )
                });
                let sin = ((sin / num_foci as u16) >> 1) as usize;
                let cos = ((cos / num_foci as u16) >> 1) as usize;
                let phase = Phase(ATAN_TABLE[(sin << 7) | cos]) + self.phase_correction(i);
                let intensity = if self.output_mask_enabled(i) {
                    Intensity(intensity)
                } else {
                    Intensity::MIN
                };
                Emission { phase, intensity }
            })
            .collect()
    }
}
