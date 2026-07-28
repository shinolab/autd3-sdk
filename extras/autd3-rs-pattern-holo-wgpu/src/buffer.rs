use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::ops::Deref;
use std::rc::Rc;

use nalgebra::Complex;

#[derive(Default)]
pub(crate) struct BufferPool {
    free: RefCell<HashMap<u64, Vec<(wgpu::Buffer, u64)>>>,
    next: Cell<u64>,
}

impl BufferPool {
    fn next_id(&self) -> u64 {
        let id = self.next.get() + 1;
        self.next.set(id);
        id
    }
}

const POOL_DEPTH: usize = 4;

pub struct Pooled {
    buf: Option<wgpu::Buffer>,
    id: u64,
    pool: Rc<BufferPool>,
}

impl Pooled {
    pub(crate) fn new(pool: &Rc<BufferPool>, buf: wgpu::Buffer) -> Self {
        Self {
            buf: Some(buf),
            id: pool.next_id(),
            pool: Rc::clone(pool),
        }
    }

    pub(crate) fn take(pool: &Rc<BufferPool>, size: u64) -> Option<Self> {
        Self::take_avoiding(pool, size, &HashSet::new())
    }

    pub(crate) fn take_avoiding(
        pool: &Rc<BufferPool>,
        size: u64,
        avoid: &HashSet<u64>,
    ) -> Option<Self> {
        let mut free = pool.free.borrow_mut();
        let slot = free.get_mut(&size)?;
        let at = slot.iter().rposition(|(_, id)| !avoid.contains(id))?;
        let (buf, id) = slot.swap_remove(at);
        Some(Self {
            buf: Some(buf),
            id,
            pool: Rc::clone(pool),
        })
    }

    pub(crate) fn id(&self) -> u64 {
        self.id
    }
}

impl Deref for Pooled {
    type Target = wgpu::Buffer;

    fn deref(&self) -> &wgpu::Buffer {
        self.buf.as_ref().expect("buffer is taken only on drop")
    }
}

impl Drop for Pooled {
    fn drop(&mut self) {
        if let Some(buf) = self.buf.take() {
            let mut free = self.pool.free.borrow_mut();
            let slot = free.entry(buf.size()).or_default();
            if slot.len() < POOL_DEPTH {
                slot.push((buf, self.id));
            }
        }
    }
}

pub(crate) struct PartialSum {
    pub(crate) buf: Pooled,
    pub(crate) chunks: u32,
}

pub struct GpuVector {
    pub(crate) buf: Pooled,
    pub(crate) len: usize,
    pub(crate) batch: usize,
    pub(crate) partial: RefCell<Option<PartialSum>>,
}

impl GpuVector {
    pub(crate) fn new(buf: Pooled, len: usize, batch: usize) -> Self {
        Self {
            buf,
            len,
            batch,
            partial: RefCell::new(None),
        }
    }

    pub(crate) fn deferred(buf: Pooled, len: usize, batch: usize, partial: PartialSum) -> Self {
        Self {
            buf,
            len,
            batch,
            partial: RefCell::new(Some(partial)),
        }
    }
}

pub struct GpuMatrix {
    pub(crate) buf: Pooled,
    pub(crate) rows: usize,
    pub(crate) cols: usize,
    pub(crate) batch: usize,
    pub(crate) row_major: bool,
}

pub(crate) fn to_raw(data: &[Complex<f32>]) -> Vec<[f32; 2]> {
    data.iter().map(|c| [c.re, c.im]).collect()
}

pub(crate) fn from_raw(raw: &[[f32; 2]]) -> Vec<Complex<f32>> {
    raw.iter().map(|v| Complex::new(v[0], v[1])).collect()
}
