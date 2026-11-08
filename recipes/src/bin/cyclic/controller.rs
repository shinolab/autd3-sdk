use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, mpsc};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, ensure};
use autd3_rs::commands::{Command, Nop};
use autd3_rs::value::SysTime;
use autd3_rs::{Controller, DeviceReply, Error, ExecAt, Frames, MAX_INFLIGHT};
use tokio::runtime::Handle;
use tokio::sync::{mpsc as report_channel, oneshot};

pub(crate) struct CycleReport {
    pub(crate) exec_at: SysTime,
    pub(crate) send_due: SysTime,
    pub(crate) sent_at: SysTime,
    pub(crate) payload_bytes: usize,
    pub(crate) replies: Vec<DeviceReply>,
}

struct Submission {
    frames: Arc<Frames>,
    index: usize,
    done: oneshot::Sender<Vec<DeviceReply>>,
}

struct InFlight {
    msg_id: u16,
    report: CycleReport,
    done: Option<oneshot::Sender<Vec<DeviceReply>>>,
}

type InFlightFrames = Arc<Mutex<VecDeque<InFlight>>>;

fn lock(in_flight: &InFlightFrames) -> MutexGuard<'_, VecDeque<InFlight>> {
    in_flight.lock().unwrap_or_else(PoisonError::into_inner)
}

pub(crate) struct CyclicController {
    controller: Arc<Controller>,
    submissions: mpsc::Sender<Submission>,
    running: Arc<AtomicBool>,
    cycle_thread: Option<JoinHandle<Result<()>>>,
    receiver: tokio::task::JoinHandle<Result<()>>,
}

impl CyclicController {
    pub(crate) fn start(
        controller: Controller,
        period: Duration,
        lead: Duration,
    ) -> Result<(Self, report_channel::UnboundedReceiver<CycleReport>)> {
        ensure!(!period.is_zero(), "the period must be longer than 0");
        ensure!(
            lead.as_nanos().div_ceil(period.as_nanos()) < MAX_INFLIGHT as u128,
            "lead {lead:?} queues more than {} frames of period {period:?} in the device FIFO",
            MAX_INFLIGHT - 1
        );
        controller.discard_unread_replies()?;
        let controller = Arc::new(controller);
        let running = Arc::new(AtomicBool::new(true));
        let in_flight = InFlightFrames::default();
        let (reports, report_receiver) = report_channel::unbounded_channel();
        let receiver = tokio::spawn(receive_replies(
            Arc::clone(&controller),
            Arc::clone(&in_flight),
            Arc::clone(&running),
            reports,
        ));
        let (submissions, submission_receiver) = mpsc::channel();
        let cycle_thread = std::thread::Builder::new()
            .name("autd3-cyclic".into())
            .spawn({
                let controller = Arc::clone(&controller);
                let running = Arc::clone(&running);
                let handle = Handle::current();
                let receiver = receiver.abort_handle();
                move || {
                    let result = run_cycles(
                        &controller,
                        &handle,
                        &submission_receiver,
                        &in_flight,
                        &running,
                        period,
                        lead,
                    );
                    receiver.abort();
                    result
                }
            })?;
        Ok((
            Self {
                controller,
                submissions,
                running,
                cycle_thread: Some(cycle_thread),
                receiver,
            },
            report_receiver,
        ))
    }

    pub(crate) async fn send(&self, cmd: impl Command) -> Result<()> {
        let frames = Arc::new(Frames::encode(self.controller.geometry(), cmd)?);
        for index in 0..frames.len() {
            let (done, replies) = oneshot::channel();
            self.submissions
                .send(Submission {
                    frames: Arc::clone(&frames),
                    index,
                    done,
                })
                .ok()
                .context("the cycle thread has stopped")?;
            let replies = replies
                .await
                .context("a device did not reply to the frame")?;
            if let Some(reply) = replies
                .iter()
                .filter(|reply| reply.status != 0)
                .min_by_key(|reply| reply.device)
            {
                return Err(Error::DeviceError {
                    device: reply.device,
                    code: reply.status,
                }
                .into());
            }
        }
        Ok(())
    }

    pub(crate) async fn stop(mut self) -> Result<()> {
        self.running.store(false, Ordering::Relaxed);
        let cycles = self
            .cycle_thread
            .take()
            .map_or(Ok(Ok(())), JoinHandle::join)
            .map_err(|_| anyhow!("the cycle thread panicked"))?;
        self.receiver.abort();
        let received = match (&mut self.receiver).await {
            Ok(received) => received,
            Err(e) if e.is_cancelled() => Ok(()),
            Err(e) => Err(e.into()),
        };
        cycles.and(received)
    }
}

impl Drop for CyclicController {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        self.receiver.abort();
    }
}

fn run_cycles(
    controller: &Controller,
    handle: &Handle,
    submissions: &mpsc::Receiver<Submission>,
    in_flight: &InFlightFrames,
    running: &AtomicBool,
    period: Duration,
    lead: Duration,
) -> Result<()> {
    let nop = Frames::encode(controller.geometry(), Nop)?;
    let first = controller.device_time_now()? + lead + period;
    let period_ns = u64::try_from(period.as_nanos())?;
    for cycle in 0u64.. {
        if !running.load(Ordering::Relaxed) {
            break;
        }
        let exec_at = SysTime::from_nanos(first.sys_time() + period_ns * cycle);
        let send_due = exec_at - lead;
        let now = controller.device_time_now()?;
        if send_due > now {
            std::thread::sleep(send_due - now);
        }
        let submission = submissions.try_recv().ok();
        let frame = match &submission {
            Some(submission) => submission.frames.frame(submission.index),
            None => nop.frame(0),
        }
        .context("the command built no frame")?;
        let payload_bytes = (0..controller.num_devices())
            .map(|device| frame.datagram_for(device).payload_len)
            .sum();
        let mut in_flight = lock(in_flight);
        let sent_at = controller.device_time_now()?;
        let msg_id = handle.block_on(controller.send_frame(frame, ExecAt::At(exec_at)))?;
        in_flight.push_back(InFlight {
            msg_id,
            report: CycleReport {
                exec_at,
                send_due,
                sent_at,
                payload_bytes,
                replies: Vec::with_capacity(controller.num_devices()),
            },
            done: submission.map(|submission| submission.done),
        });
    }
    Ok(())
}

async fn receive_replies(
    controller: Arc<Controller>,
    in_flight: InFlightFrames,
    running: Arc<AtomicBool>,
    reports: report_channel::UnboundedSender<CycleReport>,
) -> Result<()> {
    let result: Result<()> = async {
        loop {
            let reply = controller.recv().await?;
            let mut in_flight = lock(&in_flight);
            let Some(position) = in_flight.iter().position(|f| f.msg_id == reply.msg_id) else {
                continue;
            };
            let replies = &mut in_flight[position].report.replies;
            replies.push(reply);
            if replies.len() < controller.num_devices() {
                continue;
            }
            let closed: Vec<_> = in_flight.drain(..=position).collect();
            drop(in_flight);
            for InFlight { report, done, .. } in closed {
                if let Some(done) = done
                    && report.replies.len() == controller.num_devices()
                {
                    let _ = done.send(report.replies.clone());
                }
                let _ = reports.send(report);
            }
        }
    }
    .await;
    running.store(false, Ordering::Relaxed);
    result
}
