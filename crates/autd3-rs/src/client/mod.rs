mod config;
mod pool;
mod response_future;
mod rt;

#[cfg(test)]
mod tests;

pub use config::{ClientConfig, MAX_DEVICES};
pub use response_future::ResponseFuture;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, PoisonError};
use std::thread::JoinHandle;

use tokio::sync::{mpsc, oneshot};

use crate::command::Pattern;
use crate::datagram::{Datagram, DatagramBuilder, Frame};
use crate::error::{Error, PayloadError};
use crate::firmware_version::FirmwareVersion;
use crate::geometry::Geometry;
use crate::link::{IntoLink, Link};
use crate::operation::{Distribution, Synchronize};
use crate::params::{MOD_BUFFER_SAMPLES, NUM_TRANSDUCERS};
use crate::protocol::Cmd;
use crate::value::Emission;

use pool::SlotPool;
use rt::CmdMessage;

pub struct Client {
    cmd_tx: mpsc::Sender<CmdMessage>,
    num_devices: usize,
    pool: Arc<SlotPool>,
    join: std::sync::Mutex<Option<JoinHandle<()>>>,
    closed: Arc<AtomicBool>,
}

impl Client {
    pub fn open<'g, T: IntoLink + 'g>(
        geometry: &'g Geometry,
        link: T,
        config: ClientConfig,
    ) -> impl Future<Output = Result<Self, Error>> + Send + 'g {
        Box::pin(async move {
            Self::open_impl(geometry, link, config)
                .await
                .map(|(client, _checker)| client)
        })
    }

    pub fn open_with_checker<'g, T: IntoLink + 'g>(
        geometry: &'g Geometry,
        link: T,
        config: ClientConfig,
    ) -> impl Future<Output = Result<(Self, <T::Link as Link>::Checker), Error>> + Send + 'g {
        Box::pin(Self::open_impl(geometry, link, config))
    }

    async fn open_impl<T: IntoLink>(
        geometry: &Geometry,
        link: T,
        config: ClientConfig,
    ) -> Result<(Self, <T::Link as Link>::Checker), Error> {
        let config = config.validate()?;
        let link = link.into_link().await?;
        let num_devices = link.num_devices();
        if num_devices == 0 || num_devices > MAX_DEVICES {
            return Err(Error::InvalidPayload(PayloadError::DeviceCountOutOfRange {
                got: num_devices,
                max: MAX_DEVICES,
            }));
        }
        if geometry.len() != num_devices {
            return Err(Error::InvalidPayload(
                PayloadError::GeometryDeviceMismatch {
                    geometry: geometry.len(),
                    link: num_devices,
                },
            ));
        }

        let checker = link.state_checker();
        let pool = SlotPool::new(num_devices, config.max_inflight.get());

        let (cmd_tx, cmd_rx) = mpsc::channel::<CmdMessage>(1);
        let (hs_done_tx, hs_done_rx) = oneshot::channel::<Result<(), String>>();
        let closed = Arc::new(AtomicBool::new(false));
        let closed_for_rt = Arc::clone(&closed);
        let pool_for_rt = Arc::clone(&pool);

        let join = std::thread::Builder::new()
            .name("autd3-rs-rt".to_owned())
            .spawn(move || {
                rt::run_rt_thread(link, cmd_rx, pool_for_rt, config, hs_done_tx, closed_for_rt);
            })
            .map_err(|e| Error::Link(format!("failed to spawn RT thread: {e}")))?;

        match hs_done_rx.await {
            Ok(Ok(())) => {
                let client = Self {
                    cmd_tx,
                    num_devices,
                    pool,
                    join: std::sync::Mutex::new(Some(join)),
                    closed,
                };
                if let Err(e) = client.synchronize().await {
                    let _ = client.close().await;
                    return Err(e);
                }
                Ok((client, checker))
            }
            Ok(Err(msg)) => {
                let _ = wait_thread(join).await;
                Err(Error::Link(msg))
            }
            Err(_) => {
                let _ = wait_thread(join).await;
                Err(Error::RtClosed)
            }
        }
    }

    #[must_use]
    pub fn num_devices(&self) -> usize {
        self.num_devices
    }

    #[must_use]
    pub fn datagram_builder<'a>(&self) -> DatagramBuilder<'a> {
        DatagramBuilder::new(self.num_devices)
    }

    #[must_use]
    pub fn pattern_buffer(&self) -> Vec<[Emission; NUM_TRANSDUCERS]> {
        vec![[Emission::default(); NUM_TRANSDUCERS]; self.num_devices]
    }

    #[must_use]
    pub fn modulation_buffer(&self) -> Vec<u8> {
        Vec::with_capacity(MOD_BUFFER_SAMPLES)
    }

    async fn send_datagrams(&self, datagrams: &[Datagram]) -> Result<ResponseFuture, Error> {
        if datagrams.len() != self.num_devices {
            return Err(Error::InvalidPayload(PayloadError::DatagramCountMismatch {
                expected: self.num_devices,
                got: datagrams.len(),
            }));
        }
        let mut slot = self.pool.acquire().await;
        slot.reset(Distribution::PerDevice);
        for (device, datagram) in datagrams.iter().enumerate() {
            slot.payload_mut(device).copy_from_slice(&datagram.payload);
            slot.set_cmd(device, datagram.cmd);
        }
        self.dispatch(slot, false).await
    }

    async fn send_broadcast(&self, datagram: &Datagram) -> Result<ResponseFuture, Error> {
        let mut slot = self.pool.acquire().await;
        slot.reset(Distribution::Broadcast);
        slot.payload_mut(0).copy_from_slice(&datagram.payload);
        slot.set_cmd(0, datagram.cmd);
        self.dispatch(slot, false).await
    }

    async fn send_broadcast_exclusive(&self, datagram: &Datagram) -> Result<ResponseFuture, Error> {
        let mut slot = self.pool.acquire().await;
        slot.reset(Distribution::Broadcast);
        slot.payload_mut(0).copy_from_slice(&datagram.payload);
        slot.set_cmd(0, datagram.cmd);
        self.dispatch(slot, true).await
    }

    pub async fn send(&self, frame: Frame<'_>) -> Result<ResponseFuture, Error> {
        match frame.distribution() {
            Distribution::Broadcast => self.send_broadcast(&frame.datagrams()[0]).await,
            Distribution::PerDevice => self.send_datagrams(frame.datagrams()).await,
        }
    }

    pub async fn send_checked(&self, frame: Frame<'_>) -> Result<(), Error> {
        self.send(frame).await?.await?.check()
    }

    async fn dispatch(&self, slot: pool::Slot, exclusive: bool) -> Result<ResponseFuture, Error> {
        let (response_tx, response_rx) = oneshot::channel();
        if let Err(e) = self
            .cmd_tx
            .send(CmdMessage {
                frame: slot,
                response_tx,
                exclusive,
            })
            .await
        {
            self.pool.release(e.0.frame);
            return Err(Error::RtClosed);
        }
        Ok(ResponseFuture { rx: response_rx })
    }

    async fn synchronize(&self) -> Result<(), Error> {
        let datagrams = self.datagram_builder().push(Synchronize).build()?;
        for frame in &datagrams {
            self.send_checked(frame).await?;
        }
        Ok(())
    }

    pub async fn stop(&self) -> Result<(), Error> {
        let patterns = self.pattern_buffer();
        let datagrams = self
            .datagram_builder()
            .push(Pattern::new(&patterns))
            .build()?;
        for frame in &datagrams {
            self.send_checked(frame).await?;
        }
        Ok(())
    }

    pub async fn read_firmware_version(&self) -> Result<Vec<FirmwareVersion>, Error> {
        let major = self
            .send_broadcast_exclusive(&Datagram::no_payload(Cmd::ReadCpuFwVersionMajor))
            .await?
            .await?
            .data;
        let minor = self
            .send_broadcast_exclusive(&Datagram::no_payload(Cmd::ReadCpuFwVersionMinor))
            .await?
            .await?
            .data;
        let patch = self
            .send_broadcast_exclusive(&Datagram::no_payload(Cmd::ReadCpuFwVersionPatch))
            .await?
            .await?
            .data;
        Ok(major
            .into_iter()
            .zip(minor)
            .zip(patch)
            .map(|((major, minor), patch)| FirmwareVersion {
                major,
                minor,
                patch,
            })
            .collect())
    }

    pub async fn read_error_detail(&self) -> Result<Vec<u8>, Error> {
        Ok(self
            .send_broadcast_exclusive(&Datagram::no_payload(Cmd::ReadErrorDetail))
            .await?
            .await?
            .data)
    }

    pub async fn close(&self) -> Result<(), Error> {
        self.closed.store(true, Ordering::Release);
        let join = self
            .join
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(join) = join {
            wait_thread(join).await
        } else {
            Ok(())
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::Release);
        let join = self
            .join
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(join) = join {
            let _ = join.join();
        }
    }
}

async fn wait_thread(join: JoinHandle<()>) -> Result<(), Error> {
    tokio::task::spawn_blocking(move || join.join())
        .await
        .map_err(|e| Error::Link(format!("RT thread join failed: {e}")))?
        .map_err(|_| Error::Link("RT thread panicked".to_owned()))
}
