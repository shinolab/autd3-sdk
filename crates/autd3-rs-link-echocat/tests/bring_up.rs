use std::time::{Duration, Instant};

use autd3_cpu_wire::Cmd;
use autd3_rs_core::DcClock;
use autd3_rs_core::protocol::{RX_FRAME_BYTES, TX_FRAME_BYTES, TxFrame};
use autd3_rs_core::value::DcSysTime;
use autd3_rs_firmware_emulator::Device;
use autd3_rs_link_echocat::master::init::{INPUT_BYTES, OUTPUT_BYTES};
use autd3_rs_link_echocat::master::{LOSE_CONTACT_AFTER_CYCLES, frame_wire_bytes};
use autd3_rs_link_echocat::reg::AlState;
use autd3_rs_link_echocat::sim::{EscSim, ProcessData, SubDevice};
use autd3_rs_link_echocat::wire::{
    Address, Command, FrameBuilder, LOCALLY_ADMINISTERED_BIT, MIN_ETHERNET_FRAME_BYTES,
    SOURCE_MAC_OFFSET,
};
use autd3_rs_link_echocat::{Master, MasterConfig, RawBus};

const NUM_TRANSDUCERS: usize = 249;

struct FirmwareProcessData(Device);

impl ProcessData for FirmwareProcessData {
    fn exchange(&mut self, outputs: &[u8], inputs: &mut [u8]) {
        let frame: &[u8; TX_FRAME_BYTES] = outputs.try_into().expect("626 output bytes");
        let rx = self.0.send(frame);
        let mut encoded = [0u8; RX_FRAME_BYTES];
        rx.write_to(&mut encoded);
        inputs.copy_from_slice(&encoded);
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

fn open_safe_op(devices: usize) -> Master<EscSim> {
    let sim = EscSim::with_process_data(devices, Duration::from_millis(1), |_| {
        Box::new(FirmwareProcessData(Device::new(NUM_TRANSDUCERS)))
    });
    Master::open(sim, test_config()).expect("the simulated bus reaches SAFE-OP")
}

struct CountingBus {
    inner: EscSim,
    sends: usize,
}

impl CountingBus {
    fn take_sends(&mut self) -> usize {
        std::mem::take(&mut self.sends)
    }

    fn inner_mut(&mut self) -> &mut EscSim {
        &mut self.inner
    }
}

impl RawBus for CountingBus {
    fn send(&mut self, frame: &[u8]) -> std::io::Result<()> {
        self.sends += 1;
        self.inner.send(frame)
    }

    fn receive(&mut self, buf: &mut [u8], timeout: Duration) -> std::io::Result<Option<usize>> {
        self.inner.receive(buf, timeout)
    }

    fn mtu(&self) -> usize {
        self.inner.mtu()
    }
}

fn open(devices: usize) -> Master<EscSim> {
    let mut master = open_safe_op(devices);
    let tx = vec![0u8; devices * usize::from(OUTPUT_BYTES)];
    let mut rx = vec![0u8; devices * usize::from(INPUT_BYTES)];
    master.cycle(&tx, &mut rx).expect("the bus enters OP");
    master
}

#[test]
fn open_stops_at_safe_op_so_op_is_entered_under_the_caller_cadence() {
    let mut master = open_safe_op(1);
    assert!(!master.is_op());
    assert_eq!(master.bus().devices()[0].al_state(), Some(AlState::SafeOp));

    let tx = vec![0u8; usize::from(OUTPUT_BYTES)];
    let mut rx = vec![0u8; usize::from(INPUT_BYTES)];
    master.cycle(&tx, &mut rx).expect("the bus enters OP");

    assert!(master.is_op());
    assert_eq!(master.bus().devices()[0].al_state(), Some(AlState::Op));
}

#[test]
fn a_single_device_bus_reaches_op() {
    let master = open(1);
    assert_eq!(master.num_devices(), 1);
    let device = &master.bus().devices()[0];
    assert_eq!(device.al_state(), Some(AlState::Op));
    assert_eq!(device.al_status_code(), 0);
}

fn brd(bus: &mut EscSim, register: u16) -> Vec<u8> {
    let mut frame = vec![0u8; MIN_ETHERNET_FRAME_BYTES];
    let mut builder = FrameBuilder::new(&mut frame, 0);
    builder
        .push(Command::Brd, Address::broadcast(register), 2)
        .expect("the datagram fits");
    let len = builder.finish();
    bus.send(&frame[..len]).expect("the simulated bus accepts");
    let mut reply = vec![0u8; len];
    let received = bus
        .receive(&mut reply, Duration::from_millis(1))
        .expect("the simulated bus replies")
        .expect("a reply is queued");
    reply.truncate(received);
    reply
}

#[test]
fn only_the_first_device_destroys_non_ethercat_frames() {
    let master = open(3);
    let flags: Vec<bool> = master
        .bus()
        .devices()
        .iter()
        .map(SubDevice::destroys_non_ethercat_frames)
        .collect();
    assert_eq!(flags, vec![true, false, false]);
}

#[test]
fn replies_carry_the_locally_administered_bit_once_the_bus_is_brought_up() {
    let mut untouched = EscSim::nop(1, Duration::from_millis(1));
    let before = brd(&mut untouched, 0x0130);
    assert_eq!(before[SOURCE_MAC_OFFSET] & LOCALLY_ADMINISTERED_BIT, 0);

    let mut master = open(1);
    let after = brd(master.bus_mut(), 0x0130);
    assert_eq!(
        after[SOURCE_MAC_OFFSET] & LOCALLY_ADMINISTERED_BIT,
        LOCALLY_ADMINISTERED_BIT
    );
}

#[test]
fn sync_managers_and_fmmus_are_configured_from_the_esi_layout() {
    let master = open(2);
    for device in master.bus().devices() {
        let outputs = device.sync_manager(2);
        assert_eq!(outputs.start, 0x1800);
        assert_eq!(outputs.length, OUTPUT_BYTES);
        assert_eq!(outputs.control, 0x64);
        assert!(outputs.enabled);

        let inputs = device.sync_manager(3);
        assert_eq!(inputs.start, 0x1f80);
        assert_eq!(inputs.length, INPUT_BYTES);
        assert_eq!(inputs.control, 0x20);
        assert!(inputs.enabled);
    }
}

#[test]
fn sync0_is_armed_on_a_cycle_boundary_so_the_shift_is_zero() {
    let master = open(3);
    let cycle_ns = u64::try_from(master.cycle_time().as_nanos()).expect("cycle fits");
    for device in master.bus().devices() {
        assert_eq!(u64::from(device.sync0_cycle_time()), cycle_ns);
        assert_eq!(
            device.sync_start_time() % cycle_ns,
            0,
            "a non-zero SYNC0 shift is what makes AUTD3 refuse OP"
        );
    }
}

#[test]
fn propagation_delays_grow_by_one_hop_per_device() {
    let master = open(4);
    let delays: Vec<u32> = master
        .bus()
        .devices()
        .iter()
        .map(autd3_rs_link_echocat::sim::SubDevice::system_time_delay)
        .collect();
    let hop = u32::try_from(autd3_rs_link_echocat::sim::DEFAULT_HOP_NS).expect("hop fits in u32");
    assert_eq!(delays[0], 0);
    for window in delays.windows(2) {
        assert_eq!(window[1] - window[0], hop);
    }
}

#[test]
fn every_device_reads_the_same_system_time_so_sync0_fires_together() {
    let master = open(4);
    let now = master.bus().now_ns();
    let times: Vec<u64> = master
        .bus()
        .devices()
        .iter()
        .map(|device| device.system_time(now))
        .collect();
    assert!(
        times.windows(2).all(|pair| pair[0] == pair[1]),
        "system times diverge across the bus: {times:?}; \
         the propagation delay has to be folded into the system time offset"
    );
}

#[test]
fn the_bus_clock_is_aligned_to_the_host_dc_epoch() {
    let master = open(2);
    let host = autd3_rs_core::value::DcSysTime::now().unwrap().sys_time();
    let now = master.bus().now_ns();
    for (index, device) in master.bus().devices().iter().enumerate() {
        let skew = device.system_time(now).abs_diff(host);
        assert!(
            skew < Duration::from_secs(1).as_nanos().try_into().expect("fits"),
            "device {index} is {skew} ns away from the host DC clock; \
             TransitionMode::SysTime and GpioOutputType::SysTimeEq carry absolute \
             DcSysTime values, so the bus has to share that epoch"
        );
    }
}

#[test]
fn every_device_sees_its_own_slice_of_the_process_data() {
    let devices = 3;
    let mut master = open(devices);
    let mut tx = vec![0u8; devices * usize::from(OUTPUT_BYTES)];
    let mut rx = vec![0u8; devices * usize::from(INPUT_BYTES)];

    let write_seq = |tx: &mut [u8], index: usize, seq: u8| {
        let at = index * usize::from(OUTPUT_BYTES);
        let slice: &mut [u8; TX_FRAME_BYTES] = (&mut tx[at..at + usize::from(OUTPUT_BYTES)])
            .try_into()
            .expect("626 bytes");
        TxFrame::new(autd3_rs_core::protocol::Seq::new(seq), Cmd::Nop).write_to(slice);
    };

    for index in 0..devices {
        write_seq(&mut tx, index, 0);
    }
    for _ in 0..3 {
        master.cycle(&tx, &mut rx).expect("a cycle completes");
    }
    for index in 0..devices {
        assert_eq!(
            rx[index * usize::from(INPUT_BYTES)],
            0,
            "device {index} acknowledged the first sequence number"
        );
    }

    write_seq(&mut tx, 0, 1);
    let mut report = master.cycle(&tx, &mut rx).expect("a cycle completes");
    for _ in 0..3 {
        report = master.cycle(&tx, &mut rx).expect("a cycle completes");
    }
    assert!(report.rx_valid, "every device processed the cycle");

    assert_eq!(
        rx[0], 1,
        "only the device whose slice advanced acknowledges the new sequence number"
    );
    for index in 1..devices {
        assert_eq!(
            rx[index * usize::from(INPUT_BYTES)],
            0,
            "device {index} received its own slice, not the one addressed to device 0"
        );
    }
}

#[test]
fn a_dropped_frame_is_reported_as_an_invalid_cycle_and_then_recovers() {
    let devices = 2;
    let mut master = open(devices);
    let tx = vec![0u8; devices * usize::from(OUTPUT_BYTES)];
    let mut rx = vec![0u8; devices * usize::from(INPUT_BYTES)];

    assert!(
        master
            .cycle(&tx, &mut rx)
            .expect("a cycle completes")
            .rx_valid
    );

    master.bus_mut().drop_next_frames(1);
    let started = Instant::now();
    let report = master
        .cycle(&tx, &mut rx)
        .expect("a dropped frame is not an error");
    let stalled_for = started.elapsed();
    assert!(
        !report.rx_valid,
        "a lost frame must not look like a good cycle"
    );

    let report = master.cycle(&tx, &mut rx).expect("a cycle completes");
    assert!(report.rx_valid, "the bus recovers on the next cycle");

    let config = test_config();
    assert!(
        stalled_for < config.pdu_timeout / 2,
        "the lost cycle stalled for {stalled_for:?}; giving cyclic receive the acyclic \
         {:?} budget lets one lost frame outlast the {:?} SM watchdog, which turns a single \
         drop into every device falling out of OP",
        config.pdu_timeout,
        config.process_data_watchdog,
    );
}

#[test]
fn a_bus_that_stops_answering_is_reported_as_lost_instead_of_still_op() {
    let devices = 2;
    let mut master = open(devices);
    let tx = vec![0u8; devices * usize::from(OUTPUT_BYTES)];
    let mut rx = vec![0u8; devices * usize::from(INPUT_BYTES)];
    let state = master.state();
    assert!(state.all_op());

    master
        .bus_mut()
        .drop_next_frames(usize::try_from(LOSE_CONTACT_AFTER_CYCLES).expect("fits"));
    for _ in 0..LOSE_CONTACT_AFTER_CYCLES {
        assert!(
            !master
                .cycle(&tx, &mut rx)
                .expect("a silent bus is not an error")
                .rx_valid
        );
    }

    assert_eq!(
        state.states(),
        vec![autd3_rs_core::DeviceState::Lost; devices],
        "a bus that answers nothing must not keep publishing the last AL status it \
         happened to see; all_op() staying true is what stops the recovery from ever running",
    );
    assert!(!state.all_op());

    for _ in 0..devices {
        assert!(
            master
                .cycle(&tx, &mut rx)
                .expect("a cycle completes")
                .rx_valid
        );
    }
    assert_eq!(
        state.states(),
        vec![autd3_rs_core::DeviceState::Op; devices],
        "the rotation re-observes every device once the answers come back"
    );
}

#[test]
fn a_device_that_fell_out_of_op_while_silent_is_recovered_once_it_answers_again() {
    let devices = 2;
    let mut master = open(devices);
    let tx = vec![0u8; devices * usize::from(OUTPUT_BYTES)];
    let mut rx = vec![0u8; devices * usize::from(INPUT_BYTES)];
    let state = master.state();

    master
        .bus_mut()
        .drop_next_frames(usize::try_from(LOSE_CONTACT_AFTER_CYCLES).expect("fits"));
    master.bus_mut().latch_al_error(AlState::SafeOp, 0x001a);
    for _ in 0..LOSE_CONTACT_AFTER_CYCLES {
        master.cycle(&tx, &mut rx).expect("a silent bus is fine");
    }
    assert_eq!(
        state.states(),
        vec![autd3_rs_core::DeviceState::Lost; devices]
    );

    // The cycle itself carries the recovery; the caller only has to keep cycling.
    for _ in 0..8 * devices {
        master.cycle(&tx, &mut rx).expect("a cycle completes");
    }

    assert_eq!(
        state.states(),
        vec![autd3_rs_core::DeviceState::Op; devices],
        "the devices dropped to SAFE-OP with a sync error while the frames were gone; \
         the cycle has to acknowledge that and ask for OP again once they answer",
    );
    assert!(state.recoveries() > 0);
}

#[test]
fn recovering_a_device_does_not_put_another_frame_on_the_wire() {
    let devices = 4;
    let sim = EscSim::with_process_data(devices, Duration::from_millis(1), |_| {
        Box::new(FirmwareProcessData(Device::new(NUM_TRANSDUCERS)))
    });
    let mut master = Master::open(
        CountingBus {
            inner: sim,
            sends: 0,
        },
        test_config(),
    )
    .expect("the simulated bus reaches SAFE-OP");
    let tx = vec![0u8; devices * usize::from(OUTPUT_BYTES)];
    let mut rx = vec![0u8; devices * usize::from(INPUT_BYTES)];
    master.cycle(&tx, &mut rx).expect("the bus enters OP");
    let state = master.state();

    master.bus_mut().take_sends();
    for _ in 0..devices {
        master.cycle(&tx, &mut rx).expect("a cycle completes");
    }
    let healthy = master.bus_mut().take_sends();
    assert_eq!(state.recoveries(), 0);

    master
        .bus_mut()
        .inner_mut()
        .latch_al_error(AlState::SafeOp, 0x001a);
    for _ in 0..devices {
        master.cycle(&tx, &mut rx).expect("a cycle completes");
    }

    let before = state.recoveries();
    master.bus_mut().take_sends();
    for _ in 0..devices {
        master.cycle(&tx, &mut rx).expect("a cycle completes");
    }
    let recovering = master.bus_mut().take_sends();

    assert!(
        state.recoveries() > before,
        "the measured window has to contain real recovery traffic",
    );
    assert_eq!(
        recovering, healthy,
        "OP recovery rides the cyclic frames; an acyclic FPWR per device would put \
         {devices} more frames on the wire inside the cycle",
    );
}

#[test]
fn a_cycle_that_has_devices_to_recover_never_waits_out_the_pdu_timeout() {
    let devices = 4;
    let config = test_config();
    let mut master = open(devices);
    let tx = vec![0u8; devices * usize::from(OUTPUT_BYTES)];
    let mut rx = vec![0u8; devices * usize::from(INPUT_BYTES)];
    let state = master.state();

    master.bus_mut().latch_al_error(AlState::SafeOp, 0x001a);
    for _ in 0..devices {
        master.cycle(&tx, &mut rx).expect("a cycle completes");
    }
    assert!(
        !state.all_op(),
        "every device has to be known to be below OP before the bus goes quiet",
    );

    master
        .bus_mut()
        .drop_next_frames(frame_wire_bytes(devices, 1500).len());
    let started = Instant::now();
    master
        .cycle(&tx, &mut rx)
        .expect("a silent bus is not an error");
    let elapsed = started.elapsed();

    assert!(
        elapsed < config.pdu_timeout,
        "a cycle that had {devices} devices to recover took {elapsed:?}; an acyclic \
         transfer per device would block up to {:?} x {devices} inside one cycle",
        config.pdu_timeout,
    );
}

#[test]
fn a_bus_left_in_an_error_state_by_the_previous_session_is_acknowledged_on_open() {
    let devices = 2;
    let mut sim = EscSim::with_process_data(devices, Duration::from_millis(1), |_| {
        Box::new(FirmwareProcessData(Device::new(NUM_TRANSDUCERS)))
    });
    sim.latch_al_error(AlState::SafeOp, 0x001a);

    let mut master = Master::open(sim, test_config())
        .expect("a latched AL error must be acknowledged, not reported as a refused transition");

    let tx = vec![0u8; devices * usize::from(OUTPUT_BYTES)];
    let mut rx = vec![0u8; devices * usize::from(INPUT_BYTES)];
    master.cycle(&tx, &mut rx).expect("the bus enters OP");
    for device in master.bus().devices() {
        assert_eq!(device.al_state(), Some(AlState::Op));
        assert_eq!(device.al_status_code(), 0);
    }
}

#[test]
fn the_cyclic_dc_time_is_anchored_to_the_host_wall_clock() {
    let opened_at = DcSysTime::now().unwrap().sys_time();
    let mut master = open(1);

    let tx = vec![0u8; usize::from(OUTPUT_BYTES)];
    let mut rx = vec![0u8; usize::from(INPUT_BYTES)];
    let report = master.cycle(&tx, &mut rx).expect("a cycle on an OP bus");

    let skew = report.dc_system_time.abs_diff(opened_at);
    assert!(
        skew < Duration::from_secs(1).as_nanos().try_into().expect("fits"),
        "the cyclic FRMW DC time is what the link publishes as the bus clock, so it \
         has to share the host epoch; it sits {skew} ns away from it",
    );

    let clock = DcClock::new();
    clock
        .observe(DcSysTime::from_nanos(report.dc_system_time))
        .unwrap();
    assert_eq!(clock.observation().map(|o| o.samples), Some(1));
}
