use std::sync::Arc;
use std::time::Duration;

use autd3_rs::value::SysTime;
use autd3_rs::{
    Controller, Error, ExecAt, FirmwareVersion, Frames, Geometry, InflightFrames, MAX_INFLIGHT,
    Response, TransportOption,
};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

#[derive(Clone)]
pub struct Client {
    controller: Arc<Controller>,
    exclusive: Arc<Mutex<()>>,
}

pub struct Heartbeat(Option<JoinHandle<()>>);

impl Drop for Heartbeat {
    fn drop(&mut self) {
        if let Some(task) = &self.0 {
            task.abort();
        }
    }
}

async fn send_heartbeats(controller: Arc<Controller>, period: Duration) {
    let mut interval = (period != Duration::ZERO).then(|| {
        let mut interval = tokio::time::interval(period);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        interval
    });
    loop {
        if let Some(interval) = &mut interval {
            interval.tick().await;
        } else {
            tokio::task::yield_now().await;
        }
        if let Err(e) = controller.send_heartbeat().await {
            tracing::warn!("sending a heartbeat failed: {e}");
        }
    }
}

impl Client {
    pub async fn open(
        geometry: &Geometry,
        option: &TransportOption,
    ) -> Result<(Self, Heartbeat), Error> {
        let controller = Arc::new(Controller::open(geometry, option).await?);
        let heartbeat = Heartbeat(
            option
                .heartbeat_interval
                .map(|period| tokio::spawn(send_heartbeats(Arc::clone(&controller), period))),
        );
        Ok((
            Self {
                controller,
                exclusive: Arc::default(),
            },
            heartbeat,
        ))
    }

    #[must_use]
    pub fn num_devices(&self) -> usize {
        self.controller.num_devices()
    }

    #[must_use]
    pub fn geometry(&self) -> &Geometry {
        self.controller.geometry()
    }

    pub fn device_time_now(&self) -> Result<SysTime, Error> {
        self.controller.device_time_now()
    }

    pub async fn send(&self, frames: &Frames) -> Result<(), Error> {
        let _exclusive = self.exclusive.lock().await;
        let controller = &*self.controller;
        for frame in frames {
            controller.discard_unread_replies()?;
            let msg_id = controller.send_frame(frame, ExecAt::Immediate).await?;
            controller.collect(msg_id).await?.check()?;
        }
        Ok(())
    }

    pub async fn send_streaming(&self, frames: &Frames) -> Result<(), Error> {
        let _exclusive = self.exclusive.lock().await;
        let controller = &*self.controller;
        controller.discard_unread_replies()?;
        let mut inflight = InflightFrames::default();
        let mut first_error = None;
        for frame in frames {
            if frame.waits_for_previous() {
                while let Some(response) = inflight.collect_oldest(controller).await? {
                    first_error = first_error.or(response.check().err());
                }
            } else if inflight.len() >= MAX_INFLIGHT
                && let Some(response) = inflight.collect_oldest(controller).await?
            {
                first_error = first_error.or(response.check().err());
            }
            if first_error.is_some() {
                break;
            }
            match controller.send_frame(frame, ExecAt::Immediate).await {
                Ok(msg_id) => inflight.push(msg_id),
                Err(e) => {
                    first_error = Some(e);
                    break;
                }
            }
        }
        while let Some(response) = inflight.collect_oldest(controller).await? {
            first_error = first_error.or(response.check().err());
        }
        first_error.map_or(Ok(()), Err)
    }

    pub async fn read_state(&self) -> Result<Response, Error> {
        let _exclusive = self.exclusive.lock().await;
        self.controller.discard_unread_replies()?;
        let msg_id = self.controller.send_heartbeat().await?;
        self.controller.collect(msg_id).await
    }

    pub async fn read_firmware_version(&self) -> Result<Vec<FirmwareVersion>, Error> {
        let _exclusive = self.exclusive.lock().await;
        self.controller.read_firmware_version().await
    }
}
