use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, TryLockError, Weak};
use std::time::{Duration, Instant};

use autd3_python_capsule::to_pyerr;
use autd3_rs::driver::Poll;
use autd3_rs::udp::StateChecker;
use autd3_rs::{Connector as CoreConnector, Driver as CoreDriver, Error};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use crate::client::Checker;
use crate::future::{completed_into_py, future_into_py};
use crate::udp::TransportOption;

const STOP_POLL: Duration = Duration::from_millis(50);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);
const BUSY: &str = "the driver is being driven by another thread";

struct Shared {
    driver: Mutex<Option<CoreDriver>>,
    stop: AtomicBool,
}

static DRIVERS: Mutex<Vec<Weak<Shared>>> = Mutex::new(Vec::new());
static DETACHED: (Mutex<usize>, Condvar) = (Mutex::new(0), Condvar::new());

struct DetachedGuard;

impl DetachedGuard {
    fn enter() -> Self {
        *DETACHED.0.lock().unwrap_or_else(PoisonError::into_inner) += 1;
        Self
    }
}

impl Drop for DetachedGuard {
    fn drop(&mut self) {
        *DETACHED.0.lock().unwrap_or_else(PoisonError::into_inner) -= 1;
        DETACHED.1.notify_all();
    }
}

pub(crate) fn shutdown_drivers() {
    let drivers = std::mem::take(&mut *DRIVERS.lock().unwrap_or_else(PoisonError::into_inner));
    for shared in drivers.iter().filter_map(Weak::upgrade) {
        shared.stop.store(true, Ordering::Release);
        if let Ok(mut driver) = shared.driver.try_lock() {
            drop(driver.take());
        }
    }
    let count = DETACHED.0.lock().unwrap_or_else(PoisonError::into_inner);
    let _ = DETACHED
        .1
        .wait_timeout_while(count, SHUTDOWN_TIMEOUT, |count| *count > 0);
}

fn busy(py: Python<'_>) -> PyErr {
    to_pyerr(py, BUSY)
}

fn lock(shared: &Shared) -> Option<MutexGuard<'_, Option<CoreDriver>>> {
    match shared.driver.try_lock() {
        Ok(guard) => Some(guard),
        Err(TryLockError::Poisoned(poisoned)) => Some(poisoned.into_inner()),
        Err(TryLockError::WouldBlock) => None,
    }
}

fn timeout(seconds: f64) -> PyResult<Duration> {
    Duration::try_from_secs_f64(seconds)
        .map_err(|_| PyValueError::new_err(format!("invalid timeout: {seconds}")))
}

enum Outcome {
    Busy,
    Done(Result<(), Error>),
}

fn wait_until(shared: &Shared, driver: &mut CoreDriver, deadline: Instant) {
    loop {
        let now = Instant::now();
        if now >= deadline || shared.stop.load(Ordering::Acquire) {
            return;
        }
        driver.wait(deadline.min(now + STOP_POLL));
        if Instant::now() < deadline.min(now + STOP_POLL) {
            return;
        }
    }
}

#[pyclass(name = "Connector", module = "autd3")]
pub struct Connector {
    inner: Mutex<Option<CoreConnector>>,
    num_devices: usize,
}

impl Connector {
    pub(crate) fn take(&self, py: Python<'_>) -> PyResult<CoreConnector> {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
            .ok_or_else(|| to_pyerr(py, "the connector has already been used"))
    }
}

#[pymethods]
impl Connector {
    #[getter]
    fn num_devices(&self) -> usize {
        self.num_devices
    }
}

#[pyclass(name = "Driver", module = "autd3")]
pub struct Driver {
    shared: Arc<Shared>,
    checker: Arc<Mutex<StateChecker>>,
    num_devices: usize,
    #[cfg(unix)]
    fileno: i32,
    #[cfg(windows)]
    fileno: u64,
}

#[pymethods]
impl Driver {
    #[staticmethod]
    fn open(
        py: Python<'_>,
        option: &TransportOption,
        num_devices: usize,
    ) -> PyResult<(Self, Connector)> {
        let option = option.inner.clone();
        let (driver, connector) = py
            .detach(|| {
                let _guard = DetachedGuard::enter();
                CoreDriver::open(&option, num_devices)
            })
            .map_err(|e| to_pyerr(py, e))?;
        #[cfg(unix)]
        let fileno = std::os::fd::AsRawFd::as_raw_fd(&driver);
        #[cfg(windows)]
        let fileno = std::os::windows::io::AsRawSocket::as_raw_socket(&driver);
        let checker = Arc::new(Mutex::new(driver.state_checker()));
        let shared = Arc::new(Shared {
            driver: Mutex::new(Some(driver)),
            stop: AtomicBool::new(false),
        });
        let mut registry = DRIVERS.lock().unwrap_or_else(PoisonError::into_inner);
        registry.retain(|weak| weak.strong_count() > 0);
        registry.push(Arc::downgrade(&shared));
        drop(registry);
        Ok((
            Self {
                shared,
                checker,
                num_devices,
                fileno,
            },
            Connector {
                inner: Mutex::new(Some(connector)),
                num_devices,
            },
        ))
    }

    #[getter]
    fn num_devices(&self) -> usize {
        self.num_devices
    }

    fn run(&self, py: Python<'_>) -> PyResult<()> {
        let shared = Arc::clone(&self.shared);
        let outcome = py.detach(move || {
            let _guard = DetachedGuard::enter();
            let Some(mut guard) = lock(&shared) else {
                return Outcome::Busy;
            };
            let Some(driver) = guard.as_mut() else {
                return Outcome::Done(Ok(()));
            };
            while let Poll::Next(deadline) = driver.poll() {
                if shared.stop.load(Ordering::Acquire) {
                    break;
                }
                wait_until(&shared, driver, deadline);
            }
            Outcome::Done(guard.take().map_or(Ok(()), CoreDriver::close))
        });
        match outcome {
            Outcome::Busy => Err(busy(py)),
            Outcome::Done(result) => result.map_err(|e| to_pyerr(py, e)),
        }
    }

    fn poll(&self, py: Python<'_>) -> PyResult<Option<f64>> {
        let mut guard = lock(&self.shared).ok_or_else(|| busy(py))?;
        let Some(driver) = guard.as_mut() else {
            return Ok(None);
        };
        Ok(match driver.poll() {
            Poll::Next(deadline) => Some(
                deadline
                    .saturating_duration_since(Instant::now())
                    .as_secs_f64(),
            ),
            Poll::Closed => None,
        })
    }

    fn wait(&self, py: Python<'_>, timeout: f64) -> PyResult<()> {
        let timeout = self::timeout(timeout)?;
        let shared = Arc::clone(&self.shared);
        let waited = py.detach(move || {
            let _guard = DetachedGuard::enter();
            let deadline = Instant::now() + timeout;
            let Some(mut guard) = lock(&shared) else {
                return false;
            };
            if let Some(driver) = guard.as_mut() {
                wait_until(&shared, driver, deadline);
            }
            true
        });
        if waited { Ok(()) } else { Err(busy(py)) }
    }

    fn notified<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let notified = lock(&self.shared)
            .ok_or_else(|| busy(py))?
            .as_ref()
            .map(CoreDriver::notified);
        match notified {
            Some(notified) => future_into_py(py, async move {
                notified.await;
                Ok(())
            }),
            None => completed_into_py(py, ()),
        }
    }

    fn fileno(&self) -> i64 {
        #[cfg(unix)]
        {
            i64::from(self.fileno)
        }
        #[cfg(windows)]
        {
            i64::try_from(self.fileno).unwrap_or(i64::MAX)
        }
    }

    fn state_checker(&self) -> Checker {
        Checker {
            inner: Arc::clone(&self.checker),
        }
    }

    fn is_closed(&self, py: Python<'_>) -> PyResult<bool> {
        let guard = lock(&self.shared).ok_or_else(|| busy(py))?;
        Ok(guard.as_ref().is_none_or(CoreDriver::is_closed))
    }

    fn close(&self, py: Python<'_>) -> PyResult<()> {
        let driver = lock(&self.shared).ok_or_else(|| busy(py))?.take();
        match driver {
            Some(driver) => py.detach(|| driver.close()).map_err(|e| to_pyerr(py, e)),
            None => Ok(()),
        }
    }
}
