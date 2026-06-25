use std::io::Read;

use crate::TransducerLayout;

pub(crate) const MAGIC: [u8; 4] = *b"ARMT";
pub(crate) const VERSION: u8 = 2;

pub(crate) const TAG_FRAME: u8 = 0x01;
pub(crate) const TAG_CLOSE: u8 = 0x02;
pub(crate) const TAG_GEOMETRY: u8 = 0x03;

const LAYOUT_BYTES: usize = 24;

pub(crate) fn encode_geometry(layout: &[TransducerLayout]) -> Vec<u8> {
    let n = u32::try_from(layout.len()).unwrap_or(u32::MAX);
    let mut out = Vec::with_capacity(1 + 4 + layout.len() * LAYOUT_BYTES);
    out.push(TAG_GEOMETRY);
    out.extend_from_slice(&n.to_le_bytes());
    for t in layout {
        for v in t.pos.iter().chain(t.dir.iter()) {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    out
}

pub(crate) fn read_geometry(stream: &mut impl Read) -> std::io::Result<Vec<TransducerLayout>> {
    let mut n_buf = [0u8; 4];
    stream.read_exact(&mut n_buf)?;
    let n = u32::from_le_bytes(n_buf) as usize;
    let mut layout = Vec::with_capacity(n);
    let mut buf = [0u8; LAYOUT_BYTES];
    for _ in 0..n {
        stream.read_exact(&mut buf)?;
        let f = |i: usize| {
            let mut b = [0u8; 4];
            b.copy_from_slice(&buf[i * 4..i * 4 + 4]);
            f32::from_le_bytes(b)
        };
        layout.push(TransducerLayout {
            pos: [f(0), f(1), f(2)],
            dir: [f(3), f(4), f(5)],
        });
    }
    Ok(layout)
}
