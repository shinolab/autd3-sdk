use std::ffi::{CStr, c_void};
use std::ptr::NonNull;

use autd3_rs_core::Geometry;
use autd3_rs_core::value::Emission;
use pyo3::exceptions::{PyAttributeError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyCapsule, PyCapsuleMethods};

pub const GEOMETRY_CAPSULE_NAME: &CStr = c"autd3.geometry.v1";
pub const PATTERN_CAPSULE_NAME: &CStr = c"autd3.pattern.v1";
pub const PATTERN_MUT_CAPSULE_NAME: &CStr = c"autd3.pattern.mut.v1";
pub const MODULATION_CAPSULE_NAME: &CStr = c"autd3.modulation.v1";

pub type DevicePattern = Vec<Emission>;

pub fn to_pyerr<E: core::fmt::Display>(py: Python<'_>, e: E) -> PyErr {
    let msg = e.to_string();
    match py
        .import("autd3_core")
        .and_then(|m| m.getattr("Autd3Error"))
        .and_then(|c| c.call1((msg.clone(),)))
    {
        Ok(inst) => PyErr::from_value(inst),
        Err(_) => PyValueError::new_err(msg),
    }
}

pub fn to_pyerr_gil<E: core::fmt::Display>(e: E) -> PyErr {
    Python::attach(|py| to_pyerr(py, e))
}

pub fn capsule_of<'py>(obj: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyCapsule>> {
    if let Ok(capsule) = obj.cast::<PyCapsule>() {
        return Ok(capsule.clone());
    }
    let capsule = obj.call_method0("_capsule")?;
    Ok(capsule.cast_into::<PyCapsule>()?)
}

pub fn legacy_capsule_of<'py>(obj: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyCapsule>> {
    if let Ok(capsule) = obj.cast::<PyCapsule>() {
        return Ok(capsule.clone());
    }
    let capsule = match obj.call_method0("_legacy_capsule") {
        Ok(capsule) => capsule,
        Err(e) if e.is_instance_of::<PyAttributeError>(obj.py()) => {
            let name = obj
                .get_type()
                .name()
                .map_or_else(|_| "this link".to_owned(), |name| name.to_string());
            return Err(PyTypeError::new_err(format!(
                "{name} does not support LegacyClient; update the autd3-link-* wheel to one that exposes _legacy_capsule"
            )));
        }
        Err(e) => return Err(e),
    };
    Ok(capsule.cast_into::<PyCapsule>()?)
}

pub fn geometry_into_capsule(py: Python<'_>, geometry: Geometry) -> PyResult<Bound<'_, PyCapsule>> {
    PyCapsule::new_with_value(py, geometry, GEOMETRY_CAPSULE_NAME)
}

pub fn geometry_from_capsule<'a>(capsule: &'a Bound<'_, PyCapsule>) -> PyResult<&'a Geometry> {
    let ptr: NonNull<c_void> = capsule.pointer_checked(Some(GEOMETRY_CAPSULE_NAME))?;
    Ok(unsafe { ptr.cast::<Geometry>().as_ref() })
}

pub fn pattern_into_capsule(
    py: Python<'_>,
    emissions: Vec<DevicePattern>,
) -> PyResult<Bound<'_, PyCapsule>> {
    PyCapsule::new_with_value(py, emissions, PATTERN_CAPSULE_NAME)
}

pub fn pattern_from_capsule<'a>(
    capsule: &'a Bound<'_, PyCapsule>,
) -> PyResult<&'a [DevicePattern]> {
    let ptr: NonNull<c_void> = capsule.pointer_checked(Some(PATTERN_CAPSULE_NAME))?;
    Ok(unsafe { ptr.cast::<Vec<DevicePattern>>().as_ref() })
}

pub struct PatternBufferMut {
    addr: usize,
    _owner: Py<PyAny>,
}

#[allow(clippy::missing_safety_doc)]
pub unsafe fn pattern_capsule_mut(
    py: Python<'_>,
    ptr: NonNull<Vec<DevicePattern>>,
    owner: Py<PyAny>,
) -> PyResult<Bound<'_, PyCapsule>> {
    PyCapsule::new_with_value(
        py,
        PatternBufferMut {
            addr: ptr.as_ptr() as usize,
            _owner: owner,
        },
        PATTERN_MUT_CAPSULE_NAME,
    )
}

#[allow(clippy::mut_from_ref)]
pub fn pattern_from_capsule_mut<'a>(
    capsule: &'a Bound<'_, PyCapsule>,
) -> PyResult<&'a mut Vec<DevicePattern>> {
    let ptr: NonNull<c_void> = capsule.pointer_checked(Some(PATTERN_MUT_CAPSULE_NAME))?;
    let addr = unsafe { ptr.cast::<PatternBufferMut>().as_ref() }.addr;
    Ok(unsafe { &mut *(addr as *mut Vec<DevicePattern>) })
}

pub fn modulation_into_capsule(py: Python<'_>, data: Vec<u8>) -> PyResult<Bound<'_, PyCapsule>> {
    PyCapsule::new_with_value(py, data, MODULATION_CAPSULE_NAME)
}

pub fn modulation_from_capsule<'a>(capsule: &'a Bound<'_, PyCapsule>) -> PyResult<&'a [u8]> {
    let ptr: NonNull<c_void> = capsule.pointer_checked(Some(MODULATION_CAPSULE_NAME))?;
    Ok(unsafe { ptr.cast::<Vec<u8>>().as_ref() })
}

#[cfg(feature = "client")]
mod link {
    use std::cell::RefCell;
    use std::ffi::{CStr, c_void};
    use std::future::Future;
    use std::pin::Pin;
    use std::ptr::NonNull;
    use std::sync::{Arc, OnceLock};

    use autd3_rs::Error;
    use autd3_rs::{ClientConfig, Frames, Response, ResponseFuture};
    use autd3_rs_core::Geometry;
    use pyo3::exceptions::PyValueError;
    use pyo3::prelude::*;
    use pyo3::types::{PyCapsule, PyCapsuleMethods};

    pub const LINK_CAPSULE_NAME: &CStr = c"autd3.link.v1";
    pub const FRAME_CAPSULE_NAME: &CStr = c"autd3.frame.v1";

    #[must_use]
    pub fn link_runtime() -> &'static tokio::runtime::Runtime {
        static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
        RT.get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("failed to build tokio runtime")
        })
    }

    #[allow(clippy::needless_pass_by_value)]
    #[must_use]
    pub fn join_err(e: tokio::task::JoinError) -> Error {
        Error::Link(autd3_rs::LinkCause::new(e))
    }

    #[must_use]
    pub fn link_err(message: impl Into<String>) -> Error {
        autd3_rs_core::error::LinkError::new(message).into()
    }

    pub fn frame_into_capsule(
        py: Python<'_>,
        frames: Arc<Frames>,
        index: usize,
    ) -> PyResult<Bound<'_, PyCapsule>> {
        PyCapsule::new_with_value(py, (frames, index), FRAME_CAPSULE_NAME)
    }

    pub fn frame_from_capsule(capsule: &Bound<'_, PyCapsule>) -> PyResult<(Arc<Frames>, usize)> {
        let ptr: NonNull<c_void> = capsule.pointer_checked(Some(FRAME_CAPSULE_NAME))?;
        let (frames, index) = unsafe { ptr.cast::<(Arc<Frames>, usize)>().as_ref() };
        Ok((Arc::clone(frames), *index))
    }

    pub type BoxFuture<T> = Pin<Box<dyn Future<Output = Result<T, Error>> + Send>>;

    pub struct LinkStatusData {
        pub device_states: Vec<String>,
        pub all_op: bool,
        pub any_lost: bool,
        pub recoveries: u64,
    }

    pub struct ResponseToken {
        fut: ResponseFuture,
        handle: tokio::runtime::Handle,
    }

    impl ResponseToken {
        #[must_use]
        pub fn new(fut: ResponseFuture, handle: tokio::runtime::Handle) -> Self {
            Self { fut, handle }
        }

        #[must_use]
        pub fn wait(self) -> BoxFuture<Response> {
            let Self { fut, handle } = self;
            Box::pin(async move { handle.spawn(fut).await.map_err(join_err)? })
        }
    }

    pub trait ClientBackend: Send + Sync {
        fn num_devices(&self) -> usize;
        fn dc_offset_ns(&self) -> i64;
        fn read_firmware_version(&self) -> BoxFuture<Vec<String>>;
        fn read_fpga_state(&self) -> BoxFuture<Vec<u8>>;
        fn read_error_detail(&self) -> BoxFuture<Vec<u8>>;
        fn read_telemetry(&self, counter: autd3_rs::Telemetry) -> BoxFuture<Vec<u8>>;
        fn send(&self, datagrams: Arc<Frames>, index: usize) -> BoxFuture<ResponseToken>;
        fn send_checked(&self, datagrams: Arc<Frames>, frame: Option<usize>) -> BoxFuture<()>;
        fn check_status(&self) -> Result<LinkStatusData, Error>;
        fn stop(&self) -> BoxFuture<()>;
        fn close(&self) -> BoxFuture<()>;
    }

    pub type ClientOpener =
        Box<dyn FnOnce(Geometry, ClientConfig) -> BoxFuture<Box<dyn ClientBackend>> + Send>;

    pub fn client_opener<F, Fut>(f: F) -> ClientOpener
    where
        F: FnOnce(Geometry, ClientConfig) -> Fut + Send + 'static,
        Fut: Future<Output = Result<Box<dyn ClientBackend>, Error>> + Send + 'static,
    {
        Box::new(move |geo, cfg| Box::pin(f(geo, cfg)))
    }

    pub fn link_into_capsule(
        py: Python<'_>,
        opener: ClientOpener,
    ) -> PyResult<Bound<'_, PyCapsule>> {
        PyCapsule::new_with_value(py, RefCell::new(Some(opener)), LINK_CAPSULE_NAME)
    }

    pub fn take_client_opener(capsule: &Bound<'_, PyCapsule>) -> PyResult<ClientOpener> {
        let ptr: NonNull<c_void> = capsule.pointer_checked(Some(LINK_CAPSULE_NAME))?;
        let cell = unsafe { ptr.cast::<RefCell<Option<ClientOpener>>>().as_ref() };
        cell.borrow_mut()
            .take()
            .ok_or_else(|| PyValueError::new_err("link has already been consumed by open()"))
    }

    use autd3_rs::legacy::{LegacyClient, LegacyClientConfig, LegacyError, LegacyFrames};
    use autd3_rs_core::link::{IntoLink, StateCheck};

    pub const LEGACY_LINK_CAPSULE_NAME: &CStr = c"autd3.legacy_link.v1";
    pub const LEGACY_FRAME_CAPSULE_NAME: &CStr = c"autd3.legacy_frame.v1";

    pub type LegacyBoxFuture<T> = Pin<Box<dyn Future<Output = Result<T, LegacyError>> + Send>>;

    pub trait LegacyClientBackend: Send + Sync {
        fn num_devices(&self) -> usize;
        fn dc_offset_ns(&self) -> i64;
        fn read_firmware_version(&self) -> LegacyBoxFuture<Vec<String>>;
        fn read_fpga_state(&self) -> LegacyBoxFuture<Vec<u8>>;
        fn send(&self, frames: Arc<LegacyFrames>, index: usize) -> LegacyBoxFuture<Vec<u8>>;
        fn send_checked(
            &self,
            frames: Arc<LegacyFrames>,
            frame: Option<usize>,
        ) -> LegacyBoxFuture<()>;
        fn check_status(&self) -> Result<LinkStatusData, LegacyError>;
        fn stop(&self) -> LegacyBoxFuture<()>;
        fn close(&self) -> LegacyBoxFuture<()>;
    }

    pub type LegacyClientOpener = Box<
        dyn FnOnce(Geometry, LegacyClientConfig) -> LegacyBoxFuture<Box<dyn LegacyClientBackend>>
            + Send,
    >;

    #[allow(clippy::needless_pass_by_value)]
    #[must_use]
    pub fn legacy_join_err(e: tokio::task::JoinError) -> LegacyError {
        LegacyError::Link(e.to_string())
    }

    struct LegacyBackend<C> {
        client: Arc<LegacyClient>,
        checker: Arc<std::sync::Mutex<C>>,
    }

    fn legacy_frame_range(
        frames: &LegacyFrames,
        frame: Option<usize>,
    ) -> Result<(usize, usize), LegacyError> {
        match frame {
            Some(index) if index >= frames.len() => {
                Err(LegacyError::Link(format!("frame {index} out of range")))
            }
            Some(index) => Ok((index, index + 1)),
            None => Ok((0, frames.len())),
        }
    }

    impl<C: StateCheck> LegacyClientBackend for LegacyBackend<C> {
        fn num_devices(&self) -> usize {
            self.client.num_devices()
        }

        fn dc_offset_ns(&self) -> i64 {
            self.client.dc_offset_ns()
        }

        fn read_firmware_version(&self) -> LegacyBoxFuture<Vec<String>> {
            let client = Arc::clone(&self.client);
            Box::pin(async move {
                link_runtime()
                    .spawn(async move {
                        let versions = client.read_firmware_version().await?;
                        Ok::<Vec<String>, LegacyError>(
                            versions.iter().map(ToString::to_string).collect(),
                        )
                    })
                    .await
                    .map_err(legacy_join_err)?
            })
        }

        fn read_fpga_state(&self) -> LegacyBoxFuture<Vec<u8>> {
            let client = Arc::clone(&self.client);
            Box::pin(async move {
                link_runtime()
                    .spawn(async move {
                        let states = client.read_fpga_state().await?;
                        Ok::<Vec<u8>, LegacyError>(states.iter().map(|s| s.0).collect())
                    })
                    .await
                    .map_err(legacy_join_err)?
            })
        }

        fn send(&self, frames: Arc<LegacyFrames>, index: usize) -> LegacyBoxFuture<Vec<u8>> {
            let client = Arc::clone(&self.client);
            Box::pin(async move {
                link_runtime()
                    .spawn(async move {
                        let frame = frames.frame(index).ok_or_else(|| {
                            LegacyError::Link(format!("frame {index} out of range"))
                        })?;
                        Ok::<Vec<u8>, LegacyError>(client.send(frame).await?.data().to_vec())
                    })
                    .await
                    .map_err(legacy_join_err)?
            })
        }

        fn send_checked(
            &self,
            frames: Arc<LegacyFrames>,
            frame: Option<usize>,
        ) -> LegacyBoxFuture<()> {
            let client = Arc::clone(&self.client);
            Box::pin(async move {
                link_runtime()
                    .spawn(async move {
                        let (start, end) = legacy_frame_range(&frames, frame)?;
                        for index in start..end {
                            let frame = frames.frame(index).ok_or_else(|| {
                                LegacyError::Link(format!("frame {index} out of range"))
                            })?;
                            client.send_checked(frame).await?;
                        }
                        Ok::<(), LegacyError>(())
                    })
                    .await
                    .map_err(legacy_join_err)?
            })
        }

        fn check_status(&self) -> Result<LinkStatusData, LegacyError> {
            let status = self
                .checker
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .check()
                .map_err(|e| LegacyError::Link(e.to_string()))?;
            Ok(LinkStatusData {
                device_states: status.devices().iter().map(ToString::to_string).collect(),
                all_op: status.all_op(),
                any_lost: status.any_lost(),
                recoveries: status.recoveries(),
            })
        }

        fn stop(&self) -> LegacyBoxFuture<()> {
            let client = Arc::clone(&self.client);
            Box::pin(async move {
                link_runtime()
                    .spawn(async move { client.stop().await })
                    .await
                    .map_err(legacy_join_err)?
            })
        }

        fn close(&self) -> LegacyBoxFuture<()> {
            let client = Arc::clone(&self.client);
            Box::pin(async move {
                link_runtime()
                    .spawn(async move { client.close().await })
                    .await
                    .map_err(legacy_join_err)?
            })
        }
    }

    pub fn legacy_client_opener<T, F>(make_link: F) -> LegacyClientOpener
    where
        F: FnOnce(&Geometry) -> Result<T, LegacyError> + Send + 'static,
        T: IntoLink + 'static,
    {
        Box::new(move |geometry, config| {
            Box::pin(async move {
                let link = make_link(&geometry)?;
                let (client, checker) =
                    link_runtime()
                        .spawn(async move {
                            LegacyClient::open_with_checker(&geometry, link, config).await
                        })
                        .await
                        .map_err(legacy_join_err)??;
                let backend: Box<dyn LegacyClientBackend> = Box::new(LegacyBackend {
                    client: Arc::new(client),
                    checker: Arc::new(std::sync::Mutex::new(checker)),
                });
                Ok(backend)
            })
        })
    }

    pub fn legacy_link_into_capsule(
        py: Python<'_>,
        opener: LegacyClientOpener,
    ) -> PyResult<Bound<'_, PyCapsule>> {
        PyCapsule::new_with_value(py, RefCell::new(Some(opener)), LEGACY_LINK_CAPSULE_NAME)
    }

    pub fn take_legacy_client_opener(
        capsule: &Bound<'_, PyCapsule>,
    ) -> PyResult<LegacyClientOpener> {
        let ptr: NonNull<c_void> = capsule.pointer_checked(Some(LEGACY_LINK_CAPSULE_NAME))?;
        let cell = unsafe { ptr.cast::<RefCell<Option<LegacyClientOpener>>>().as_ref() };
        cell.borrow_mut()
            .take()
            .ok_or_else(|| PyValueError::new_err("link has already been consumed by open()"))
    }

    pub fn legacy_frame_into_capsule(
        py: Python<'_>,
        frames: Arc<LegacyFrames>,
        index: usize,
    ) -> PyResult<Bound<'_, PyCapsule>> {
        PyCapsule::new_with_value(py, (frames, index), LEGACY_FRAME_CAPSULE_NAME)
    }

    pub fn legacy_frame_from_capsule(
        capsule: &Bound<'_, PyCapsule>,
    ) -> PyResult<(Arc<LegacyFrames>, usize)> {
        let ptr: NonNull<c_void> = capsule.pointer_checked(Some(LEGACY_FRAME_CAPSULE_NAME))?;
        let (frames, index) = unsafe { ptr.cast::<(Arc<LegacyFrames>, usize)>().as_ref() };
        Ok((Arc::clone(frames), *index))
    }
}

#[cfg(feature = "client")]
pub use link::{
    BoxFuture, ClientBackend, ClientOpener, FRAME_CAPSULE_NAME, LEGACY_FRAME_CAPSULE_NAME,
    LEGACY_LINK_CAPSULE_NAME, LINK_CAPSULE_NAME, LegacyBoxFuture, LegacyClientBackend,
    LegacyClientOpener, LinkStatusData, ResponseToken, client_opener, frame_from_capsule,
    frame_into_capsule, join_err, legacy_client_opener, legacy_frame_from_capsule,
    legacy_frame_into_capsule, legacy_join_err, legacy_link_into_capsule, link_err,
    link_into_capsule, link_runtime, take_client_opener, take_legacy_client_opener,
};
