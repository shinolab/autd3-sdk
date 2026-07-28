use std::collections::VecDeque;
use std::io;
use std::time::Duration;

use autd3_rs_link_echocat::master::init::{INPUT_BYTES, OUTPUT_BYTES};
use autd3_rs_link_echocat::sim::EscSim;
use autd3_rs_link_echocat::{Master, MasterConfig, RawBus};

struct LoopbackBus {
    inner: EscSim,
    echo: VecDeque<Vec<u8>>,
}

impl LoopbackBus {
    fn new(devices: usize, cycle: Duration) -> Self {
        Self {
            inner: EscSim::nop(devices, cycle),
            echo: VecDeque::new(),
        }
    }
}

impl RawBus for LoopbackBus {
    fn send(&mut self, frame: &[u8]) -> io::Result<()> {
        self.echo.push_back(frame.to_vec());
        self.inner.send(frame)
    }

    fn receive(&mut self, buf: &mut [u8], timeout: Duration) -> io::Result<Option<usize>> {
        if let Some(frame) = self.echo.pop_front() {
            if frame.len() > buf.len() {
                return Err(io::Error::other("looped back frame exceeds the buffer"));
            }
            buf[..frame.len()].copy_from_slice(&frame);
            return Ok(Some(frame.len()));
        }
        self.inner.receive(buf, timeout)
    }

    fn mtu(&self) -> usize {
        self.inner.mtu()
    }

    fn echoes_sent_frames(&self) -> bool {
        true
    }
}

fn test_config() -> MasterConfig {
    MasterConfig {
        cycle: Duration::from_millis(1),
        dc_static_sync_iterations: 32,
        dc_start_delay: Duration::from_millis(10),
        ..MasterConfig::default()
    }
}

#[test]
fn an_interface_that_loops_sent_frames_back_still_reaches_op_and_exchanges_data() {
    let devices = 2;
    let bus = LoopbackBus::new(devices, Duration::from_millis(1));
    let mut master =
        Master::open(bus, test_config()).expect("a looped back frame is not mistaken for a reply");

    let tx = vec![0u8; devices * usize::from(OUTPUT_BYTES)];
    let mut rx = vec![0u8; devices * usize::from(INPUT_BYTES)];
    let mut report = master.cycle(&tx, &mut rx).expect("the bus enters OP");
    for _ in 0..3 {
        report = master.cycle(&tx, &mut rx).expect("a cycle completes");
    }

    assert!(
        report.rx_valid,
        "the loopback copy carries a zero working counter, so accepting it would poison the cycle"
    );
    assert!(master.is_op());
}
