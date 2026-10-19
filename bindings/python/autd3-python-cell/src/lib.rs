use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicUsize, Ordering};

const EXCLUSIVE: usize = usize::MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum BufferBusy {
    #[error("the buffer is being written elsewhere")]
    Writing,
    #[error("the buffer is being read elsewhere")]
    Reading,
}

#[cfg(feature = "pyo3")]
impl From<BufferBusy> for pyo3::PyErr {
    fn from(e: BufferBusy) -> Self {
        pyo3::exceptions::PyRuntimeError::new_err(e.to_string())
    }
}

#[repr(C)]
pub struct BufferCell<T> {
    state: AtomicUsize,
    value: UnsafeCell<Vec<Vec<T>>>,
}

unsafe impl<T: Send + Sync> Sync for BufferCell<T> {}

impl<T> BufferCell<T> {
    #[must_use]
    pub const fn new(value: Vec<Vec<T>>) -> Self {
        Self {
            state: AtomicUsize::new(0),
            value: UnsafeCell::new(value),
        }
    }

    pub fn try_read(&self) -> Result<SharedGuard<'_, T>, BufferBusy> {
        let mut state = self.state.load(Ordering::Relaxed);
        loop {
            if state >= EXCLUSIVE - 1 {
                return Err(BufferBusy::Writing);
            }
            match self.state.compare_exchange_weak(
                state,
                state + 1,
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                Ok(_) => return Ok(SharedGuard { cell: self }),
                Err(current) => state = current,
            }
        }
    }

    pub fn try_write(&self) -> Result<ExclusiveGuard<'_, T>, BufferBusy> {
        match self
            .state
            .compare_exchange(0, EXCLUSIVE, Ordering::Acquire, Ordering::Relaxed)
        {
            Ok(_) => Ok(ExclusiveGuard { cell: self }),
            Err(EXCLUSIVE) => Err(BufferBusy::Writing),
            Err(_) => Err(BufferBusy::Reading),
        }
    }
}

pub struct SharedGuard<'a, T> {
    cell: &'a BufferCell<T>,
}

impl<T> Deref for SharedGuard<'_, T> {
    type Target = Vec<Vec<T>>;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.cell.value.get() }
    }
}

impl<T> Drop for SharedGuard<'_, T> {
    fn drop(&mut self) {
        self.cell.state.fetch_sub(1, Ordering::Release);
    }
}

pub struct ExclusiveGuard<'a, T> {
    cell: &'a BufferCell<T>,
}

impl<T> Deref for ExclusiveGuard<'_, T> {
    type Target = Vec<Vec<T>>;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.cell.value.get() }
    }
}

impl<T> DerefMut for ExclusiveGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.cell.value.get() }
    }
}

impl<T> Drop for ExclusiveGuard<'_, T> {
    fn drop(&mut self) {
        self.cell.state.store(0, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell() -> BufferCell<u8> {
        BufferCell::new(vec![vec![0; 4]; 2])
    }

    #[test]
    fn readers_share_the_buffer() {
        let cell = cell();
        let a = cell.try_read().unwrap();
        let b = cell.try_read().unwrap();
        assert_eq!(a.len(), b.len());
    }

    #[test]
    fn a_writer_excludes_readers_and_writers() {
        let cell = cell();
        let mut w = cell.try_write().unwrap();
        w[0][0] = 1;
        assert_eq!(cell.try_read().err(), Some(BufferBusy::Writing));
        assert_eq!(cell.try_write().err(), Some(BufferBusy::Writing));
    }

    #[test]
    fn a_reader_excludes_writers() {
        let cell = cell();
        let r = cell.try_read().unwrap();
        assert_eq!(cell.try_write().err(), Some(BufferBusy::Reading));
        drop(r);
        assert!(cell.try_write().is_ok());
    }

    #[test]
    fn releasing_the_writer_reopens_the_buffer() {
        let cell = cell();
        let mut w = cell.try_write().unwrap();
        w[1][3] = 7;
        drop(w);
        assert_eq!(cell.try_read().unwrap()[1][3], 7);
        assert!(cell.try_write().is_ok());
    }
}
