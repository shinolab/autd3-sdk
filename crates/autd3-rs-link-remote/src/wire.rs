use std::io::Read;

use crate::{DeviceLayout, TransducerLayout};

pub(crate) const MAGIC: [u8; 4] = *b"ARMT";
pub(crate) const VERSION: u8 = 3;

pub(crate) const TAG_FRAME: u8 = 0x01;
pub(crate) const TAG_CLOSE: u8 = 0x02;

const LAYOUT_BYTES: usize = 24;

pub(crate) fn encode_geometry(devices: &[DeviceLayout]) -> Vec<u8> {
    let num_devices = u32::try_from(devices.len()).unwrap_or(u32::MAX);
    let total: usize = devices.iter().map(|d| d.transducers.len()).sum();
    let mut out = Vec::with_capacity(4 + devices.len() * 4 + total * LAYOUT_BYTES);
    out.extend_from_slice(&num_devices.to_le_bytes());
    for dev in devices {
        let n = u32::try_from(dev.transducers.len()).unwrap_or(u32::MAX);
        out.extend_from_slice(&n.to_le_bytes());
        for t in &dev.transducers {
            for v in t.pos.iter().chain(t.dir.iter()) {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
    }
    out
}

pub(crate) fn read_geometry(stream: &mut impl Read) -> std::io::Result<Vec<DeviceLayout>> {
    let mut u32_buf = [0u8; 4];
    stream.read_exact(&mut u32_buf)?;
    let num_devices = u32::from_le_bytes(u32_buf) as usize;
    let mut devices = Vec::with_capacity(num_devices);
    let mut buf = [0u8; LAYOUT_BYTES];
    for _ in 0..num_devices {
        stream.read_exact(&mut u32_buf)?;
        let n = u32::from_le_bytes(u32_buf) as usize;
        let mut transducers = Vec::with_capacity(n);
        for _ in 0..n {
            stream.read_exact(&mut buf)?;
            let f = |i: usize| {
                let mut b = [0u8; 4];
                b.copy_from_slice(&buf[i * 4..i * 4 + 4]);
                f32::from_le_bytes(b)
            };
            transducers.push(TransducerLayout {
                pos: [f(0), f(1), f(2)],
                dir: [f(3), f(4), f(5)],
            });
        }
        devices.push(DeviceLayout { transducers });
    }
    Ok(devices)
}
