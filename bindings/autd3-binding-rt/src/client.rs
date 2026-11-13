use std::collections::VecDeque;
use std::mem::DropGuard;
use std::sync::{Arc, PoisonError};
use std::time::Duration;

use autd3_rs::value::SysTime;
use autd3_rs::{
    Controller, DeviceReply, Error, ExecAt, FirmwareVersion, Frames, Geometry, MAX_INFLIGHT,
    Response, TransportOption,
};
use tokio::sync::{Mutex, Notify};
use tokio::task::JoinHandle;

#[derive(Clone)]
pub struct Client {
    controller: Arc<Controller>,
    shared: Arc<Shared>,
}

#[derive(Default)]
struct Shared {
    book: std::sync::Mutex<Book>,
    changed: Notify,
    sending: Mutex<()>,
    receiving: Mutex<()>,
}

#[derive(Default)]
struct Book {
    pending: VecDeque<PendingFrame>,
    device_errors: Vec<(u64, Error)>,
    next_call: u64,
}

struct PendingFrame {
    msg_id: u16,
    call: u64,
    replied: Vec<bool>,
    device_error: Option<Error>,
}

impl Book {
    fn begin_call(&mut self) -> u64 {
        self.next_call += 1;
        self.next_call
    }

    fn push(&mut self, msg_id: u16, call: u64, num_devices: usize) {
        self.pending.push_back(PendingFrame {
            msg_id,
            call,
            replied: vec![false; num_devices],
            device_error: None,
        });
    }

    fn record(&mut self, reply: &DeviceReply) {
        let Some(index) = self.pending.iter().position(|f| f.msg_id == reply.msg_id) else {
            return;
        };
        let frame = &mut self.pending[index];
        let Some(replied) = frame.replied.get_mut(reply.device) else {
            return;
        };
        if *replied {
            return;
        }
        *replied = true;
        if reply.status != 0
            && frame.device_error.as_ref().is_none_or(
                |e| matches!(e, Error::DeviceError { device, .. } if reply.device < *device),
            )
        {
            frame.device_error = Some(Error::DeviceError {
                device: reply.device,
                code: reply.status,
            });
        }
        if frame.replied.iter().all(|&r| r) {
            let frame = self
                .pending
                .remove(index)
                .expect("the index was found above");
            if let Some(error) = frame.device_error
                && !self
                    .device_errors
                    .iter()
                    .any(|(call, _)| *call == frame.call)
            {
                self.device_errors.push((frame.call, error));
            }
        }
    }

    fn has_pending(&self, call: u64) -> bool {
        self.pending.iter().any(|f| f.call == call)
    }

    fn has_error(&self, call: u64) -> bool {
        self.device_errors.iter().any(|(c, _)| *c == call)
    }

    fn take_error(&mut self, call: u64) -> Option<Error> {
        let index = self.device_errors.iter().position(|(c, _)| *c == call)?;
        Some(self.device_errors.swap_remove(index).1)
    }

    fn forget(&mut self, call: u64) {
        self.pending.retain(|f| f.call != call);
        self.device_errors.retain(|(c, _)| *c != call);
    }
}

impl Shared {
    fn book(&self) -> std::sync::MutexGuard<'_, Book> {
        self.book.lock().unwrap_or_else(PoisonError::into_inner)
    }
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
                shared: Arc::default(),
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
        self.send_frames(frames, true).await
    }

    pub async fn send_streaming(&self, frames: &Frames) -> Result<(), Error> {
        self.send_frames(frames, false).await
    }

    async fn send_frames(&self, frames: &Frames, every_frame_waits: bool) -> Result<(), Error> {
        let shared = &*self.shared;
        let call = shared.book().begin_call();
        let _registration = DropGuard::new(call, |call| {
            shared.book().forget(call);
            shared.changed.notify_waiters();
        });
        let sent = {
            let _sending = shared.sending.lock().await;
            if shared.book().pending.is_empty() {
                let _receiving = shared.receiving.lock().await;
                self.controller.discard_unread_replies()?;
            }
            let mut sent = Ok(());
            for frame in frames {
                if every_frame_waits || frame.waits_for_previous() {
                    self.wait_until(|book| book.pending.is_empty()).await?;
                } else {
                    self.wait_until(|book| book.pending.len() < MAX_INFLIGHT)
                        .await?;
                }
                if shared.book().has_error(call) {
                    break;
                }
                match self.controller.send_frame(frame, ExecAt::Immediate).await {
                    Ok(msg_id) => shared.book().push(msg_id, call, self.num_devices()),
                    Err(e) => {
                        sent = Err(e);
                        break;
                    }
                }
            }
            sent
        };
        self.wait_until(|book| !book.has_pending(call)).await?;
        sent?;
        shared.book().take_error(call).map_or(Ok(()), Err)
    }

    async fn wait_until(&self, done: impl Fn(&Book) -> bool) -> Result<(), Error> {
        let shared = &*self.shared;
        loop {
            let changed = shared.changed.notified();
            if done(&shared.book()) {
                return Ok(());
            }
            let receiving = tokio::select! {
                receiving = shared.receiving.lock() => receiving,
                () = changed => continue,
            };
            let changed = shared.changed.notified();
            if done(&shared.book()) {
                return Ok(());
            }
            let reply = tokio::select! {
                reply = self.controller.recv() => reply,
                () = changed => continue,
            };
            let recorded = reply.map(|reply| shared.book().record(&reply));
            drop(receiving);
            shared.changed.notify_waiters();
            recorded?;
        }
    }

    pub async fn read_state(&self) -> Result<Response, Error> {
        let _sending = self.shared.sending.lock().await;
        self.wait_until(|book| book.pending.is_empty()).await?;
        let _receiving = self.shared.receiving.lock().await;
        self.controller.discard_unread_replies()?;
        let msg_id = self.controller.send_heartbeat().await?;
        self.controller.collect(msg_id).await
    }

    pub async fn read_firmware_version(&self) -> Result<Vec<FirmwareVersion>, Error> {
        let _sending = self.shared.sending.lock().await;
        self.wait_until(|book| book.pending.is_empty()).await?;
        let _receiving = self.shared.receiving.lock().await;
        self.controller.read_firmware_version().await
    }
}

#[cfg(test)]
mod tests {
    use autd3_rs::udp::Flags;
    use autd3_rs::{FpgaState, Telemetry};

    use super::*;

    fn reply(device: usize, msg_id: u16, status: u8) -> DeviceReply {
        DeviceReply {
            device,
            msg_id,
            status,
            flags: Flags::ASSIGNED,
            unit_id: 0,
            sys_time: 0,
            rx_sys_time: 0,
            telemetry: Telemetry::default(),
            fpga_state: FpgaState::default(),
            data: Vec::new(),
        }
    }

    #[test]
    fn a_frame_leaves_the_book_once_every_device_replied() {
        let mut book = Book::default();
        let call = book.begin_call();
        book.push(7, call, 2);

        book.record(&reply(0, 7, 0));
        book.record(&reply(0, 7, 0));
        assert!(book.has_pending(call));
        book.record(&reply(1, 7, 0));

        assert!(!book.has_pending(call));
        assert!(book.take_error(call).is_none());
    }

    #[test]
    fn replies_to_frames_not_in_the_book_are_ignored() {
        let mut book = Book::default();
        let call = book.begin_call();
        book.push(1, call, 1);

        book.record(&reply(0, 2, 0x05));
        book.record(&reply(3, 1, 0x05));

        assert!(book.has_pending(call));
        assert!(!book.has_error(call));
    }

    #[test]
    fn a_device_error_goes_to_the_call_that_sent_the_frame_and_names_the_lowest_device() {
        let mut book = Book::default();
        let first = book.begin_call();
        let second = book.begin_call();
        book.push(1, first, 3);
        book.push(2, second, 3);

        for device in [2, 1, 0] {
            book.record(&reply(device, 1, if device == 0 { 0 } else { 0x07 }));
            book.record(&reply(device, 2, 0));
        }

        assert!(!book.has_error(second));
        assert!(matches!(
            book.take_error(first),
            Some(Error::DeviceError {
                device: 1,
                code: 0x07
            })
        ));
        assert!(book.take_error(first).is_none());
    }

    #[test]
    fn a_call_keeps_its_first_completed_error() {
        let mut book = Book::default();
        let call = book.begin_call();
        book.push(1, call, 1);
        book.push(2, call, 1);

        book.record(&reply(0, 1, 0x03));
        book.record(&reply(0, 2, 0x04));

        assert!(matches!(
            book.take_error(call),
            Some(Error::DeviceError { code: 0x03, .. })
        ));
    }

    #[test]
    fn forgetting_a_call_frees_its_frames_but_not_the_others() {
        let mut book = Book::default();
        let gone = book.begin_call();
        let kept = book.begin_call();
        book.push(1, gone, 1);
        book.push(2, kept, 1);

        book.forget(gone);
        book.record(&reply(0, 1, 0x05));

        assert_eq!(book.pending.len(), 1);
        assert!(book.has_pending(kept));
        assert!(!book.has_error(gone));
    }
}
