use std::sync::Arc;
use std::time::Duration;

use autd3_rs_core::MAX_INFLIGHT;
use tokio::task::JoinHandle;

use crate::commands::Command;
use crate::controller::{Controller, ExecAt, InflightFrames};
use crate::datagram::Frames;
use crate::error::Error;
use crate::firmware_version::FirmwareVersion;
use crate::geometry::Geometry;
use crate::response::Response;
use crate::udp::TransportOption;
use crate::value::SysTime;

pub struct Client {
    controller: Arc<Controller>,
    heartbeat: Option<JoinHandle<()>>,
}

impl Client {
    pub async fn open(geometry: &Geometry, option: &TransportOption) -> Result<Self, Error> {
        Ok(Self::new(
            Arc::new(Controller::open(geometry, option).await?),
            option.heartbeat_interval,
        ))
    }

    fn new(controller: Arc<Controller>, heartbeat_interval: Option<Duration>) -> Self {
        let heartbeat = heartbeat_interval.map(|period| {
            let controller = Arc::clone(&controller);
            tokio::spawn(async move {
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
            })
        });
        Self {
            controller,
            heartbeat,
        }
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

    pub fn send<C: Command>(
        &mut self,
        cmd: C,
    ) -> impl Future<Output = Result<(), Error>> + use<'_, C> {
        self.controller.send(cmd)
    }

    pub fn send_streaming<C: Command>(
        &mut self,
        cmd: C,
    ) -> impl Future<Output = Result<(), Error>> + use<'_, C> {
        let frames = Frames::encode(self.geometry(), cmd);
        async move {
            let frames = frames?;
            let controller = &*self.controller;
            controller.discard_unread_replies()?;
            let mut inflight = InflightFrames::default();
            let mut first_error = None;
            for frame in &frames {
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
    }

    pub async fn read_state(&mut self) -> Result<Response, Error> {
        self.controller.discard_unread_replies()?;
        let msg_id = self.controller.send_heartbeat().await?;
        self.controller.collect(msg_id).await
    }

    pub async fn read_firmware_version(&mut self) -> Result<Vec<FirmwareVersion>, Error> {
        self.controller.read_firmware_version().await
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        if let Some(heartbeat) = &self.heartbeat {
            heartbeat.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use autd3_cpu_wire::udp::{Flags, Kind};
    use tokio::net::UdpSocket;
    use zerocopy::FromBytes;

    use crate::commands::Modulation;
    use crate::controller::tests::{next_request, open_controller, reply};
    use crate::protocol::Cmd;
    use crate::value::SamplingConfig;

    use super::*;

    async fn open_client(heartbeat_interval: Option<Duration>) -> (Client, UdpSocket) {
        let (controller, device) = open_controller().await;
        (
            Client::new(Arc::new(controller), heartbeat_interval),
            device,
        )
    }

    fn assert_nothing_more_was_sent(device: &UdpSocket) {
        let mut buf = [0; 2048];
        assert!(matches!(
            device.try_recv_from(&mut buf),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock
        ));
    }

    #[tokio::test]
    async fn send_streaming_does_not_activate_a_bank_whose_write_was_rejected() {
        let (mut client, device) = open_client(None).await;
        let data = [0u8; 4];
        let modulation = Modulation::new(SamplingConfig::FREQ_4K, &data);

        let answer = async {
            let (write, host) = next_request(&device).await;
            let (config, _) = next_request(&device).await;
            reply(
                &device,
                host,
                write.msg_id.get(),
                0x07,
                Flags::ASSIGNED,
                &[],
            )
            .await;
            reply(&device, host, config.msg_id.get(), 0, Flags::ASSIGNED, &[]).await;
        };
        let (sent, ()) = tokio::join!(client.send_streaming(modulation), answer);

        assert!(matches!(
            sent,
            Err(Error::DeviceError {
                device: 0,
                code: 0x07
            })
        ));
        assert_nothing_more_was_sent(&device);
    }

    #[tokio::test]
    async fn send_streaming_activates_the_bank_after_the_writes_complete() {
        let (mut client, device) = open_client(None).await;
        let data = [0u8; 4];
        let modulation = Modulation::new(SamplingConfig::FREQ_4K, &data);

        let answer = async {
            let mut cmds = Vec::new();
            for _ in 0..2 {
                let (header, host) = next_request(&device).await;
                reply(&device, host, header.msg_id.get(), 0, Flags::ASSIGNED, &[]).await;
            }
            let mut buf = [0; 2048];
            let (_, host) = device.recv_from(&mut buf).await.unwrap();
            let (header, rest) = autd3_cpu_wire::udp::Header::read_from_prefix(&buf).unwrap();
            cmds.push(rest[1]);
            reply(&device, host, header.msg_id.get(), 0, Flags::ASSIGNED, &[]).await;
            cmds
        };
        let (sent, cmds) = tokio::join!(client.send_streaming(modulation), answer);

        sent.unwrap();
        assert_eq!(cmds, [Cmd::ActivateModulationBank.as_u8()]);
        assert_nothing_more_was_sent(&device);
    }

    #[tokio::test]
    async fn read_state_returns_the_replies_to_its_own_heartbeat() {
        let (mut client, device) = open_client(None).await;

        let answer = async {
            let (header, host) = next_request(&device).await;
            assert_eq!(header.kind, Kind::Heartbeat.as_u8());
            reply(
                &device,
                host,
                header.msg_id.get().wrapping_sub(1),
                0x01,
                Flags::ASSIGNED,
                &[],
            )
            .await;
            reply(
                &device,
                host,
                header.msg_id.get(),
                0x02,
                Flags::ASSIGNED,
                &[],
            )
            .await;
        };
        let (response, ()) = tokio::join!(client.read_state(), answer);

        assert_eq!(response.unwrap().replies()[0].status, 0x02);
    }

    #[tokio::test]
    async fn no_heartbeat_task_runs_by_default() {
        let (client, _device) = open_client(None).await;

        assert!(client.heartbeat.is_none());
    }

    #[tokio::test]
    async fn heartbeats_go_out_every_interval_until_the_client_is_dropped() {
        let (controller, device) = open_controller().await;
        tokio::time::pause();
        let start = tokio::time::Instant::now();
        let client = Client::new(Arc::new(controller), Some(Duration::from_millis(10)));

        let mut sent_at = Vec::new();
        for _ in 0..3 {
            let (header, _) = next_request(&device).await;
            assert_eq!(header.kind, Kind::Heartbeat.as_u8());
            sent_at.push(tokio::time::Instant::now() - start);
        }
        drop(client);
        let mut buf = [0; 2048];
        while device.try_recv_from(&mut buf).is_ok() {}
        tokio::time::advance(Duration::from_millis(100)).await;
        tokio::task::yield_now().await;

        for (tick, at) in (0u64..).zip(sent_at) {
            assert!(
                (Duration::from_millis(10 * tick)..Duration::from_millis(10 * tick + 2))
                    .contains(&at),
                "{at:?}"
            );
        }
        assert_nothing_more_was_sent(&device);
    }

    #[tokio::test]
    async fn a_zero_interval_sends_heartbeats_without_waiting_for_the_clock() {
        let (controller, device) = open_controller().await;
        tokio::time::pause();
        let start = tokio::time::Instant::now();
        let client = Client::new(Arc::new(controller), Some(Duration::ZERO));

        for _ in 0..3 {
            let (header, _) = next_request(&device).await;
            assert_eq!(header.kind, Kind::Heartbeat.as_u8());
        }

        assert_eq!(tokio::time::Instant::now() - start, Duration::ZERO);
        drop(client);
    }
}
