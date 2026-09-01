use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use autd3_cpu_wire::{Cmd, Telemetry};
use autd3_rs_core::protocol::{RX_FRAME_BYTES, Seq, TX_FRAME_BYTES, TxFrame};
use autd3_rs_firmware_emulator::Device;
use autd3_rs_link_echocat::master::budget::WireTiming;
use autd3_rs_link_echocat::master::init::{INPUT_BYTES, OUTPUT_BYTES};
use autd3_rs_link_echocat::sim::{EscSim, ProcessData};
use autd3_rs_link_echocat::{Master, MasterConfig};

const NUM_TRANSDUCERS: usize = 249;

struct Counted {
    exchanges: Arc<AtomicU64>,
}

impl ProcessData for Counted {
    fn exchange(&mut self, _outputs: &[u8], _inputs: &mut [u8]) {
        self.exchanges.fetch_add(1, Ordering::Relaxed);
    }
}

struct Firmware(Arc<Mutex<Device>>);

impl ProcessData for Firmware {
    fn exchange(&mut self, outputs: &[u8], inputs: &mut [u8]) {
        let frame: &[u8; TX_FRAME_BYTES] = outputs.try_into().expect("626 output bytes");
        let mut device = self.0.lock().expect("the emulator is not poisoned");
        let rx = device.send(frame);
        let mut encoded = [0u8; RX_FRAME_BYTES];
        rx.write_to(&mut encoded);
        inputs.copy_from_slice(&encoded);
    }
}

fn config(cycle: Duration) -> MasterConfig {
    MasterConfig {
        cycle,
        dc_static_sync_iterations: 32,
        dc_start_delay: Duration::from_millis(10),
        ..MasterConfig::default()
    }
}

fn open<F>(devices: usize, cycle: Duration, factory: F) -> Master<EscSim>
where
    F: FnMut(usize) -> Box<dyn ProcessData>,
{
    let mut sim = EscSim::with_process_data(devices, cycle, factory);
    sim.set_wire_timing(Some(WireTiming::default()));
    let mut master = Master::open(sim, config(cycle)).expect("the simulated bus reaches SAFE-OP");
    let tx = vec![0u8; devices * usize::from(OUTPUT_BYTES)];
    let mut rx = vec![0u8; devices * usize::from(INPUT_BYTES)];
    master.cycle(&tx, &mut rx).expect("the bus enters OP");
    master
}

fn run(master: &mut Master<EscSim>, devices: usize, cycles: usize) {
    let mut tx = vec![0u8; devices * usize::from(OUTPUT_BYTES)];
    let mut rx = vec![0u8; devices * usize::from(INPUT_BYTES)];
    for cycle in 0..cycles {
        let seq = Seq::new(u8::try_from(cycle % 256).expect("fits in u8"));
        for index in 0..devices {
            let at = index * usize::from(OUTPUT_BYTES);
            let slice: &mut [u8; TX_FRAME_BYTES] = (&mut tx[at..at + usize::from(OUTPUT_BYTES)])
                .try_into()
                .expect("626 bytes");
            TxFrame::new(seq, Cmd::Nop).write_to(slice);
        }
        master.cycle(&tx, &mut rx).expect("a cycle completes");
    }
}

#[test]
fn twenty_devices_take_about_eleven_hundred_microseconds_on_the_wire() {
    let devices = 20;
    let cycle = Duration::from_millis(2);
    let mut master = open(devices, cycle, |_| {
        Box::new(autd3_rs_link_echocat::sim::NopProcessData)
    });
    run(&mut master, devices, 4);

    assert_eq!(
        master.bus().last_exchange_wire_time(),
        Duration::from_nanos(1_060_480),
        "20 devices split into nine frames and cross 40 hops",
    );
}

#[test]
fn the_exchange_time_grows_with_the_device_count() {
    for (devices, expected_ns) in [(4usize, 217_920u64), (10, 535_440), (20, 1_060_480)] {
        let cycle = Duration::from_millis(4);
        let mut master = open(devices, cycle, |_| {
            Box::new(autd3_rs_link_echocat::sim::NopProcessData)
        });
        run(&mut master, devices, 4);

        let measured = master.bus().last_exchange_wire_time();
        assert_eq!(
            measured,
            Duration::from_nanos(expected_ns),
            "{devices} devices spent {measured:?} on the wire",
        );
    }
}

#[test]
fn a_period_shorter_than_the_exchange_fires_sync0_more_often_than_frames_arrive() {
    let devices = 20;
    let cycle = Duration::from_millis(1);
    let exchanges = Arc::new(AtomicU64::new(0));
    let counter = Arc::clone(&exchanges);
    let mut master = open(devices, cycle, move |_| {
        Box::new(Counted {
            exchanges: Arc::clone(&counter),
        })
    });

    let wire = master.bus().last_exchange_wire_time();
    assert!(
        wire > cycle,
        "the premise of this test is that {wire:?} does not fit in {cycle:?}",
    );

    let cycles = 64;
    run(&mut master, devices, cycles);

    let sync0 = master.bus().devices()[0].sync0_count();
    assert!(
        sync0 > u64::try_from(cycles).expect("fits"),
        "SYNC0 fired {sync0} times for {cycles} exchanges; the period was not oversubscribed",
    );
    assert!(exchanges.load(Ordering::Relaxed) > 0);
}

#[test]
fn an_oversubscribed_period_makes_the_firmware_reread_the_same_frame() {
    let devices = 20;
    let cycle = Duration::from_millis(1);
    let first = Arc::new(Mutex::new(Device::new(NUM_TRANSDUCERS)));
    let handle = Arc::clone(&first);
    let mut master = open(devices, cycle, move |index| {
        if index == 0 {
            Box::new(Firmware(Arc::clone(&handle)))
        } else {
            Box::new(autd3_rs_link_echocat::sim::NopProcessData)
        }
    });

    run(&mut master, devices, 64);

    let device = first.lock().expect("the emulator is not poisoned");
    let dedup = device.telemetry(Telemetry::Dedup);
    let processed = device.telemetry(Telemetry::Processed);
    assert!(
        dedup > 0,
        "the firmware never re-read a stale buffer (processed {processed}, dedup {dedup})",
    );
}

#[test]
fn a_period_that_fits_the_exchange_does_not_starve_the_firmware() {
    let devices = 20;
    let cycle = Duration::from_millis(4);
    let first = Arc::new(Mutex::new(Device::new(NUM_TRANSDUCERS)));
    let handle = Arc::clone(&first);
    let mut master = open(devices, cycle, move |index| {
        if index == 0 {
            Box::new(Firmware(Arc::clone(&handle)))
        } else {
            Box::new(autd3_rs_link_echocat::sim::NopProcessData)
        }
    });

    run(&mut master, devices, 16);

    let device = first.lock().expect("the emulator is not poisoned");
    let processed = device.telemetry(Telemetry::Processed);
    assert!(
        processed > 0,
        "the firmware processed nothing at a period that fits the exchange",
    );
}
