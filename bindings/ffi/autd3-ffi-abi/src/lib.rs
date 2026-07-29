use std::ffi::{CString, c_char, c_void};

use autd3_rs_core::value::Emission;

pub type DevicePattern = Vec<Emission>;

#[repr(transparent)]
pub struct PatternBuffer(pub Vec<DevicePattern>);

#[repr(transparent)]
pub struct ModulationBuffer(pub Vec<u8>);

#[must_use]
pub fn into_handle<T>(value: T) -> *mut T {
    Box::into_raw(Box::new(value))
}

pub unsafe fn drop_handle<T>(ptr: *mut T) {
    if !ptr.is_null() {
        drop(unsafe { Box::from_raw(ptr) });
    }
}

#[must_use]
pub fn alloc_cstring(s: &str) -> *mut c_char {
    let bytes: Vec<u8> = s.bytes().map(|b| if b == 0 { b' ' } else { b }).collect();

    CString::new(bytes).unwrap_or_default().into_raw()
}

pub unsafe fn free_cstring(ptr: *mut c_char) {
    if !ptr.is_null() {
        drop(unsafe { CString::from_raw(ptr) });
    }
}

pub type CompletionCallback =
    extern "C" fn(code: i32, value: *mut c_void, msg: *const c_char, user_data: *mut c_void);

pub struct CompletionCtx {
    cb: CompletionCallback,
    user_data: *mut c_void,
}

unsafe impl Send for CompletionCtx {}

impl CompletionCtx {
    #[must_use]
    pub fn new(cb: CompletionCallback, user_data: *mut c_void) -> Self {
        Self { cb, user_data }
    }

    pub fn ok(self, value: *mut c_void) {
        (self.cb)(0, value, std::ptr::null(), self.user_data);
    }

    pub fn err(self, message: &str) {
        let msg = CString::new(message.replace('\0', " ")).unwrap_or_default();
        (self.cb)(-1, std::ptr::null_mut(), msg.as_ptr(), self.user_data);
    }
}

#[cfg(feature = "client")]
mod client {
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::{Arc, OnceLock};
    use std::time::Duration;

    use autd3_rs::Error;
    use autd3_rs::{ClientConfig, Frames, Response, ResponseFuture, Telemetry};
    use autd3_rs_core::Geometry;
    use autd3_rs_core::link::DeviceState;

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
        Error::Link(e.to_string())
    }

    #[must_use]
    pub fn to_ns(d: Duration) -> u64 {
        u64::try_from(d.as_nanos()).unwrap_or(u64::MAX)
    }

    pub type BoxFuture<T> = Pin<Box<dyn Future<Output = Result<T, Error>> + Send>>;

    pub struct LinkStatusData {
        pub devices: Vec<DeviceState>,
        pub recoveries: u64,
    }

    pub fn merge_response(merged: &mut [u8], response: &[u8]) {
        merged
            .iter_mut()
            .zip(response.iter().copied())
            .for_each(|(m, d)| {
                if *m == 0 {
                    *m = d;
                }
            });
    }

    pub struct ResponseTokenData(pub BoxFuture<Response>);

    impl ResponseTokenData {
        #[must_use]
        pub fn from_futures(futures: Vec<ResponseFuture>) -> Self {
            Self(Box::pin(async move {
                let mut merged: Option<Response> = None;
                for future in futures {
                    let response = future.await?;
                    match merged.as_mut() {
                        None => merged = Some(response),
                        Some(m) => merge_response(m.data_mut(), response.data()),
                    }
                }
                Ok(merged.unwrap_or_default())
            }))
        }
    }

    pub trait CheckerBackend: Send + Sync {
        fn check(&self) -> BoxFuture<LinkStatusData>;
    }

    pub trait ClientBackend: Send + Sync {
        fn num_devices(&self) -> usize;
        fn dc_offset_ns(&self) -> i64;
        fn read_firmware_version(&self) -> BoxFuture<Vec<String>>;
        fn read_fpga_state(&self) -> BoxFuture<Vec<u8>>;
        fn read_error_detail(&self) -> BoxFuture<Vec<u8>>;
        fn read_telemetry(&self, counter: Telemetry) -> BoxFuture<Vec<u8>>;

        fn send(
            &self,
            datagrams: Arc<Frames>,
            frame: Option<usize>,
        ) -> BoxFuture<ResponseTokenData>;
        fn send_checked(&self, datagrams: Arc<Frames>, frame: Option<usize>) -> BoxFuture<()>;
        fn checker(&self) -> Box<dyn CheckerBackend>;
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

    use autd3_rs::legacy::{LegacyClient, LegacyClientConfig, LegacyError, LegacyFrames};
    use autd3_rs_core::link::{IntoLink, StateCheck};

    pub type LegacyBoxFuture<T> = Pin<Box<dyn Future<Output = Result<T, LegacyError>> + Send>>;

    pub trait LegacyClientBackend: Send + Sync {
        fn num_devices(&self) -> usize;
        fn dc_offset_ns(&self) -> i64;
        fn read_firmware_version(&self) -> LegacyBoxFuture<Vec<String>>;
        fn read_fpga_state(&self) -> LegacyBoxFuture<Vec<u8>>;
        fn send(&self, frames: Arc<LegacyFrames>, frame: Option<usize>)
        -> LegacyBoxFuture<Vec<u8>>;
        fn send_checked(
            &self,
            frames: Arc<LegacyFrames>,
            frame: Option<usize>,
        ) -> LegacyBoxFuture<()>;
        fn checker(&self) -> Box<dyn CheckerBackend>;
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
        checker: Arc<tokio::sync::Mutex<C>>,
    }

    struct LegacyChecker<C>(Arc<tokio::sync::Mutex<C>>);

    impl<C: StateCheck> CheckerBackend for LegacyChecker<C> {
        fn check(&self) -> BoxFuture<LinkStatusData> {
            let checker = Arc::clone(&self.0);
            Box::pin(async move {
                link_runtime()
                    .spawn(async move {
                        let status = checker
                            .lock()
                            .await
                            .check()
                            .await
                            .map_err(|e| Error::Link(e.to_string()))?;
                        Ok::<LinkStatusData, Error>(LinkStatusData {
                            devices: status.devices,
                            recoveries: status.recoveries,
                        })
                    })
                    .await
                    .map_err(join_err)?
            })
        }
    }

    fn frame_range(
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

        fn send(
            &self,
            frames: Arc<LegacyFrames>,
            frame: Option<usize>,
        ) -> LegacyBoxFuture<Vec<u8>> {
            let client = Arc::clone(&self.client);
            Box::pin(async move {
                link_runtime()
                    .spawn(async move {
                        let (start, end) = frame_range(&frames, frame)?;
                        let mut merged: Option<Vec<u8>> = None;
                        for index in start..end {
                            let frame = frames.frame(index).ok_or_else(|| {
                                LegacyError::Link(format!("frame {index} out of range"))
                            })?;
                            let data = client.send(frame).await?.data().to_vec();
                            match merged.as_mut() {
                                None => merged = Some(data),
                                Some(m) => merge_response(m, &data),
                            }
                        }
                        Ok::<Vec<u8>, LegacyError>(merged.unwrap_or_default())
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
                        let (start, end) = frame_range(&frames, frame)?;
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

        fn checker(&self) -> Box<dyn CheckerBackend> {
            Box::new(LegacyChecker(Arc::clone(&self.checker)))
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
                    checker: Arc::new(tokio::sync::Mutex::new(checker)),
                });
                Ok(backend)
            })
        })
    }

    #[cfg(test)]
    mod tests {
        use super::merge_response;

        fn merge_all(responses: &[&[u8]]) -> Vec<u8> {
            let mut merged: Option<Vec<u8>> = None;
            for response in responses {
                match merged.as_mut() {
                    None => merged = Some(response.to_vec()),
                    Some(m) => merge_response(m, response),
                }
            }
            merged.unwrap_or_default()
        }

        #[test]
        fn every_frame_contributes_to_the_merged_response() {
            assert_eq!(
                vec![1, 2, 3],
                merge_all(&[&[1, 0, 0], &[0, 2, 0], &[0, 0, 3]])
            );
        }

        #[test]
        fn the_first_non_zero_byte_wins() {
            assert_eq!(vec![1, 5], merge_all(&[&[0, 5], &[1, 6], &[2, 7]]));
        }

        #[test]
        fn a_shorter_response_leaves_the_tail_untouched() {
            assert_eq!(vec![1, 2, 0], merge_all(&[&[0, 0, 0], &[1, 2]]));
        }

        #[test]
        fn no_frame_merges_to_an_empty_response() {
            assert_eq!(Vec::<u8>::new(), merge_all(&[]));
        }
    }
}

#[cfg(feature = "client")]
pub use client::{
    BoxFuture, CheckerBackend, ClientBackend, ClientOpener, LegacyBoxFuture, LegacyClientBackend,
    LegacyClientOpener, LinkStatusData, ResponseTokenData, client_opener, join_err,
    legacy_client_opener, legacy_join_err, link_runtime, to_ns,
};
