use std::sync::{Arc, Mutex};

use autd3_python_capsule::{
    ClientBackend, ResponseToken, capsule_of, geometry_from_capsule, take_client_opener,
    to_pyerr_gil,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3_async_runtimes::tokio::future_into_py;

use crate::config::ClientConfig;
use crate::datagram::{DatagramBuilder, Frame};

#[pyclass(name = "LinkStatus", module = "autd3")]
pub struct LinkStatus {
    #[pyo3(get)]
    device_states: Vec<String>,
    #[pyo3(get)]
    all_op: bool,
    #[pyo3(get)]
    any_lost: bool,
    #[pyo3(get)]
    recoveries: u64,
}

#[pymethods]
impl LinkStatus {
    fn __eq__(&self, other: &Bound<'_, PyAny>) -> bool {
        other
            .extract::<PyRef<'_, Self>>()
            .is_ok_and(|o| self.device_states == o.device_states && self.recoveries == o.recoveries)
    }

    fn __repr__(&self) -> String {
        format!(
            "LinkStatus(devices={:?}, all_op={}, any_lost={}, recoveries={})",
            self.device_states, self.all_op, self.any_lost, self.recoveries
        )
    }
}

#[pyclass(name = "FpgaState", module = "autd3")]
pub struct FpgaState(autd3_rs::FpgaState);

#[pymethods]
impl FpgaState {
    fn raw(&self) -> u8 {
        self.0.raw()
    }

    fn is_thermal_asserted(&self) -> bool {
        self.0.is_thermal_asserted()
    }

    fn reads_enabled(&self) -> bool {
        self.0.reads_enabled()
    }

    fn __repr__(&self) -> String {
        format!(
            "FpgaState(raw=0x{:02X}, thermal_asserted={}, reads_enabled={})",
            self.0.raw(),
            self.0.is_thermal_asserted(),
            self.0.reads_enabled()
        )
    }
}

#[pyclass(name = "Client", module = "autd3")]
pub struct Client {
    backend: Arc<dyn ClientBackend>,
}

#[pymethods]
impl Client {
    #[staticmethod]
    fn open<'py>(
        py: Python<'py>,
        geometry: &Bound<'py, PyAny>,
        link: &Bound<'py, PyAny>,
        config: &ClientConfig,
    ) -> PyResult<Bound<'py, PyAny>> {
        let geometry = geometry_from_capsule(&capsule_of(geometry)?)?.clone();
        let opener = take_client_opener(&capsule_of(link)?)?;
        let config = config.inner;
        future_into_py(py, async move {
            let backend = opener(geometry, config).await.map_err(to_pyerr_gil)?;
            Ok(Client {
                backend: Arc::from(backend),
            })
        })
    }

    #[staticmethod]
    fn open_with_checker<'py>(
        py: Python<'py>,
        geometry: &Bound<'py, PyAny>,
        link: &Bound<'py, PyAny>,
        config: &ClientConfig,
    ) -> PyResult<Bound<'py, PyAny>> {
        let geometry = geometry_from_capsule(&capsule_of(geometry)?)?.clone();
        let opener = take_client_opener(&capsule_of(link)?)?;
        let config = config.inner;
        future_into_py(py, async move {
            let backend: Arc<dyn ClientBackend> =
                Arc::from(opener(geometry, config).await.map_err(to_pyerr_gil)?);
            Ok((
                Client {
                    backend: Arc::clone(&backend),
                },
                Checker { backend },
            ))
        })
    }

    fn num_devices(&self) -> usize {
        self.backend.num_devices()
    }

    fn datagram_builder(&self) -> DatagramBuilder {
        DatagramBuilder::with_devices(self.backend.num_devices())
    }

    fn read_firmware_version<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let backend = Arc::clone(&self.backend);
        future_into_py(py, async move {
            backend.read_firmware_version().await.map_err(to_pyerr_gil)
        })
    }

    fn read_fpga_state<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let backend = Arc::clone(&self.backend);
        future_into_py(py, async move {
            let states = backend.read_fpga_state().await.map_err(to_pyerr_gil)?;
            Ok(states
                .into_iter()
                .map(|s| FpgaState(autd3_rs::FpgaState(s)))
                .collect::<Vec<_>>())
        })
    }

    fn read_error_detail<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let backend = Arc::clone(&self.backend);
        future_into_py(py, async move {
            backend.read_error_detail().await.map_err(to_pyerr_gil)
        })
    }

    fn send<'py>(&self, py: Python<'py>, frame: PyRef<'_, Frame>) -> PyResult<Bound<'py, PyAny>> {
        let backend = Arc::clone(&self.backend);
        let datagrams = Arc::clone(&frame.datagrams);
        let index = frame.index;
        future_into_py(py, async move {
            let token = backend.send(datagrams, index).await.map_err(to_pyerr_gil)?;
            Ok(ResponseFuture {
                token: Mutex::new(Some(token)),
            })
        })
    }

    fn send_checked<'py>(
        &self,
        py: Python<'py>,
        frame: PyRef<'_, Frame>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let backend = Arc::clone(&self.backend);
        let datagrams = Arc::clone(&frame.datagrams);
        let index = frame.index;
        future_into_py(py, async move {
            backend
                .send_checked(datagrams, Some(index))
                .await
                .map_err(to_pyerr_gil)
        })
    }

    fn stop<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let backend = Arc::clone(&self.backend);
        future_into_py(
            py,
            async move { backend.stop().await.map_err(to_pyerr_gil) },
        )
    }

    fn close<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let backend = Arc::clone(&self.backend);
        future_into_py(
            py,
            async move { backend.close().await.map_err(to_pyerr_gil) },
        )
    }
}

#[pyclass(name = "ResponseFuture", module = "autd3")]
pub struct ResponseFuture {
    token: Mutex<Option<ResponseToken>>,
}

#[pymethods]
impl ResponseFuture {
    fn __await__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let token = self
            .token
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
            .ok_or_else(|| PyValueError::new_err("ResponseFuture has already been awaited"))?;
        let awaitable =
            future_into_py(py, async move { token.check().await.map_err(to_pyerr_gil) })?;
        awaitable.getattr("__await__")?.call0()
    }
}

#[pyclass(name = "Checker", module = "autd3")]
pub struct Checker {
    backend: Arc<dyn ClientBackend>,
}

#[pymethods]
impl Checker {
    fn check<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let backend = Arc::clone(&self.backend);
        future_into_py(py, async move {
            let status = backend.check_status().await.map_err(to_pyerr_gil)?;
            Ok(LinkStatus {
                device_states: status.device_states,
                all_op: status.all_op,
                any_lost: status.any_lost,
                recoveries: status.recoveries,
            })
        })
    }
}
