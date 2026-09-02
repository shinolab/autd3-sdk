#[cfg(target_os = "linux")]
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
#[cfg(target_os = "linux")]
use std::time::{Duration, Instant};

use autd3_rs_appliance::{CaptureRequest, CaptureState, CaptureStatus};
#[cfg(target_os = "linux")]
use pcap_file::pcap::{PcapPacket, PcapWriter};

#[cfg(target_os = "linux")]
const READ_TIMEOUT: Duration = Duration::from_millis(200);
#[cfg(target_os = "linux")]
const MAX_FRAME_BYTES: usize = 2048;

pub struct CaptureJob {
    status: Mutex<CaptureStatus>,
    stop: AtomicBool,
    path: PathBuf,
}

impl CaptureJob {
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self {
            status: Mutex::new(CaptureStatus::default()),
            stop: AtomicBool::new(false),
            path,
        }
    }

    pub fn status(&self) -> CaptureStatus {
        self.lock().clone()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn stop(&self) -> bool {
        if self.lock().state != CaptureState::Running {
            return false;
        }
        self.stop.store(true, Ordering::Release);
        true
    }

    fn lock(&self) -> MutexGuard<'_, CaptureStatus> {
        self.status.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn begin(&self, interface: &str) -> bool {
        let mut status = self.lock();
        if status.state == CaptureState::Running {
            return false;
        }
        self.stop.store(false, Ordering::Release);
        *status = CaptureStatus {
            state: CaptureState::Running,
            interface: interface.to_owned(),
            ..CaptureStatus::default()
        };
        true
    }

    #[cfg(target_os = "linux")]
    fn finish(&self, reason: &str) {
        let mut status = self.lock();
        status.state = CaptureState::Finished;
        status.stopped_because = Some(reason.to_owned());
    }

    fn fail(&self, error: String) {
        let mut status = self.lock();
        status.state = CaptureState::Failed;
        status.error = Some(error);
    }
}

#[cfg(target_os = "linux")]
pub fn run(job: &CaptureJob, interface: &str, request: CaptureRequest) -> Result<(), String> {
    use autd3_rs_link_echocat::bus::CaptureSocket;

    if !job.begin(interface) {
        return Err("a capture is already running".to_owned());
    }
    let socket = match CaptureSocket::open(interface) {
        Ok(socket) => socket,
        Err(e) => {
            job.fail(format!("cannot capture on {interface}: {e}"));
            return Err(format!("cannot capture on {interface}: {e}"));
        }
    };
    if let Some(dir) = job.path().parent()
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        job.fail(format!("cannot create {}: {e}", dir.display()));
        return Err(e.to_string());
    }
    let writer = File::create(job.path())
        .map_err(|e| e.to_string())
        .and_then(|file| PcapWriter::new(file).map_err(|e| e.to_string()));
    let mut writer = match writer {
        Ok(writer) => writer,
        Err(e) => {
            job.fail(format!("cannot write {}: {e}", job.path().display()));
            return Err(e);
        }
    };

    let started = Instant::now();
    let limit = Duration::from_secs(request.max_seconds);
    let mut buf = vec![0u8; MAX_FRAME_BYTES];
    let mut frames = 0u64;
    let mut bytes = 0u64;

    let reason = loop {
        if job.stop.load(Ordering::Acquire) {
            break "stopped on request";
        }
        let elapsed = started.elapsed();
        if elapsed >= limit {
            break "reached max_seconds";
        }
        if bytes >= request.max_bytes {
            break "reached max_bytes";
        }
        match socket.receive(&mut buf, READ_TIMEOUT) {
            Ok(Some(len)) => {
                let len = len.min(buf.len());
                let packet =
                    PcapPacket::new(elapsed, u32::try_from(len).unwrap_or(u32::MAX), &buf[..len]);
                if let Err(e) = writer.write_packet(&packet) {
                    job.fail(format!("cannot write {}: {e}", job.path().display()));
                    return Err(e.to_string());
                }
                frames += 1;
                bytes += len as u64;
            }
            Ok(None) => {}
            Err(e) => {
                job.fail(format!("capture on {interface} failed: {e}"));
                return Err(e.to_string());
            }
        }
        let mut status = job.lock();
        status.frames = frames;
        status.bytes = bytes;
        status.elapsed_seconds = elapsed.as_secs();
    };

    {
        let mut status = job.lock();
        status.frames = frames;
        status.bytes = bytes;
        status.elapsed_seconds = started.elapsed().as_secs();
    }
    job.finish(reason);
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn run(job: &CaptureJob, interface: &str, _request: CaptureRequest) -> Result<(), String> {
    let message = "capturing is only implemented on Linux".to_owned();
    job.begin(interface);
    job.fail(message.clone());
    Err(message)
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::{CaptureJob, run};
    use autd3_rs_appliance::{CaptureRequest, CaptureState};
    use autd3_rs_link_echocat::wire::ETHERTYPE_ETHERCAT;
    use autd3_rs_link_echocat::{RawBus, RawSocket};
    use pcap_file::pcap::PcapReader;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    const SENT_FRAMES: usize = 8;

    fn wait_until_running(job: &CaptureJob) -> bool {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if job.status().state == CaptureState::Running {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    #[test]
    #[ignore = "needs CAP_NET_RAW: run under `unshare -rn`"]
    fn a_capture_writes_a_readable_pcap_of_the_ethercat_frames() {
        let path = std::env::temp_dir().join("autd3-appliance-capture-test.pcap");
        let _ = std::fs::remove_file(&path);
        let job = Arc::new(CaptureJob::new(path.clone()));

        let worker = Arc::clone(&job);
        let handle = std::thread::spawn(move || {
            run(
                &worker,
                "lo",
                CaptureRequest {
                    max_bytes: 1024 * 1024,
                    max_seconds: 10,
                },
            )
        });
        assert!(wait_until_running(&job), "the capture thread never started");

        let mut sender = RawSocket::open("lo").expect("open lo");
        let mut frame = [0u8; 60];
        frame[0..6].copy_from_slice(&[0xff; 6]);
        frame[6..12].copy_from_slice(&[0x10; 6]);
        frame[12..14].copy_from_slice(&ETHERTYPE_ETHERCAT.to_be_bytes());
        for index in 0..SENT_FRAMES {
            frame[14] = u8::try_from(index).expect("index fits in u8");
            sender.send(&frame).expect("send");
            std::thread::sleep(Duration::from_millis(20));
        }

        assert!(job.stop(), "the capture was running so it can be stopped");
        handle
            .join()
            .expect("the thread joins")
            .expect("capture ok");

        let status = job.status();
        assert_eq!(status.state, CaptureState::Finished);
        assert_eq!(
            status.stopped_because.as_deref(),
            Some("stopped on request")
        );
        assert!(
            status.frames >= SENT_FRAMES as u64,
            "every frame that crossed the interface is recorded, got {}",
            status.frames
        );

        let file = std::fs::File::open(&path).expect("the capture file exists");
        let mut reader = PcapReader::new(file).expect("it is a valid pcap");
        let mut packets = 0u64;
        while let Some(packet) = reader.next_packet() {
            let packet = packet.expect("the packet parses");
            assert_eq!(
                u16::from_be_bytes([packet.data[12], packet.data[13]]),
                ETHERTYPE_ETHERCAT,
                "only EtherCAT frames are kept"
            );
            packets += 1;
        }
        assert_eq!(packets, status.frames);

        let _ = std::fs::remove_file(&path);
    }
}
