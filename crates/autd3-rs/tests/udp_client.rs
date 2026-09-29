use autd3_rs::commands::{Modulation, Pattern, SetSilencer};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::udp::emulator::UdpEmulator;
use autd3_rs::value::{Intensity, Phase, SamplingConfig};
use autd3_rs::{Client, ClientConfig, DeviceState};

#[tokio::test(flavor = "multi_thread")]
async fn a_client_drives_the_emulated_chain() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let geometry = Geometry::new(vec![Autd3::default(), Autd3::default()]);
    let (client, mut checker) =
        Client::open_with_checker(&geometry, emulator.option(), ClientConfig::default())
            .await
            .unwrap();
    assert_eq!(client.num_devices(), 2);
    assert!(client.clock_offset_ns().abs() < 1_000_000_000);

    let mut phases = geometry.phase_buffer();
    for (d, device_phases) in phases.iter_mut().enumerate() {
        for (i, p) in device_phases.iter_mut().enumerate() {
            *p = Phase(u8::try_from((i + d * 7) % 256).unwrap());
        }
    }
    let modulation: Vec<u8> = (0..16).map(|i| i * 16).collect();

    let mut builder = client.datagram_builder();
    builder
        .push(SetSilencer::default())
        .push(Pattern::new(&phases, Intensity::MAX))
        .push(Modulation::new(SamplingConfig::FREQ_4K, &modulation));
    let datagrams = builder.build().unwrap();
    for frame in &datagrams {
        client.send_checked(frame).await.unwrap();
    }

    for (d, expected) in phases.iter().enumerate() {
        let (emitted, intensities, buffer) = emulator.with_device(d, |device| {
            let fpga = device.fpga();
            let (p, i) = fpga.emissions();
            (p, i, fpga.modulation_buffer(fpga.current_mod_bank()))
        });
        assert_eq!(&emitted, expected);
        assert!(intensities.iter().all(|&i| i == Intensity::MAX));
        assert_eq!(buffer, modulation);
    }

    let status = checker.check().unwrap();
    assert_eq!(status.devices(), [DeviceState::Ready; 2]);

    client.close().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_full_modulation_buffer_streams_through_the_emulated_chain() {
    let emulator = UdpEmulator::spawn(2).unwrap();
    let geometry = Geometry::new(vec![Autd3::default(), Autd3::default()]);
    let client = Client::open(&geometry, emulator.option(), ClientConfig::default())
        .await
        .unwrap();

    let modulation: Vec<u8> = (0..autd3_rs::params::MOD_BUFFER_SAMPLES)
        .map(|i| u8::try_from((i * 7) % 251).unwrap())
        .collect();
    let datagrams = client
        .datagram_builder()
        .push(Modulation::new(SamplingConfig::FREQ_4K, &modulation))
        .build()
        .unwrap();
    let mut pending = std::collections::VecDeque::new();
    for frame in &datagrams {
        pending.push_back(client.send(frame).await.unwrap());
    }
    for future in pending {
        future.await.unwrap().check().unwrap();
    }

    for d in 0..2 {
        let buffer = emulator.with_device(d, |device| {
            let fpga = device.fpga();
            fpga.modulation_buffer(fpga.current_mod_bank())
        });
        assert_eq!(buffer, modulation);
    }
    assert_eq!(client.bus_stats().retransmissions(), 0);
    client.close().await.unwrap();
}
