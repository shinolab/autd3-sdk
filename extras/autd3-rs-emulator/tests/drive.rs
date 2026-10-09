use std::time::Duration;

use autd3_rs::commands::{FixedCompletionTime, Modulation, Pattern, SetSilencer};
use autd3_rs::common::ULTRASOUND_PERIOD;
use autd3_rs::error::Error;
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::protocol::DeviceErrorCode;
use autd3_rs::value::{Intensity, Phase, SamplingConfig};

use autd3_rs_emulator::{ClientApi, Emulator, EmulatorError};

#[test]
fn records_phase_passthrough_with_silencer_disabled() {
    let emulator = Emulator::new(Geometry::new(vec![Autd3::default()]));
    let phases = vec![vec![Phase(0x20); Autd3::NUM_TRANSDUCERS]];
    let intensities = vec![vec![Intensity::MAX; Autd3::NUM_TRANSDUCERS]];

    let record = emulator
        .record(async move |r| {
            r.send(SetSilencer::new(FixedCompletionTime {
                intensity: ULTRASOUND_PERIOD,
                phase: ULTRASOUND_PERIOD,
                strict_mode: false,
            }))
            .await?;
            r.send(Pattern::new(&phases, &intensities)).await?;
            r.tick(2 * ULTRASOUND_PERIOD)?;
            Ok(())
        })
        .unwrap();

    assert_eq!(record.num_transducers(), Autd3::NUM_TRANSDUCERS);
    assert_eq!(record.num_samples(), 2);
    assert_eq!(record.start().sys_time(), 0);
    assert_eq!(
        record.end().sys_time(),
        u64::try_from(2 * ULTRASOUND_PERIOD.as_nanos()).unwrap()
    );
    let phase = record.phase();
    assert_eq!(phase.shape(), (Autd3::NUM_TRANSDUCERS, 2));
    for col in phase.columns() {
        assert!(col.u8().unwrap().into_no_null_iter().all(|v| v == 0x20));
    }
}

#[test]
fn transducer_table_shape() {
    let emulator = Emulator::new(Geometry::new(vec![Autd3::default(), Autd3::default()]));
    let table = emulator.transducer_table();
    assert_eq!(table.height(), 2 * Autd3::NUM_TRANSDUCERS);
    assert_eq!(table.width(), 8);
}

#[test]
fn tick_must_be_multiple_of_ultrasound_period() {
    let emulator = Emulator::new(Geometry::new(vec![Autd3::default()]));
    let result = emulator.record(async move |r| {
        r.tick(Duration::from_nanos(1))?;
        Ok(())
    });
    assert!(result.is_err());
}

#[test]
fn send_reports_a_frame_the_firmware_rejects() {
    let emulator = Emulator::new(Geometry::new(vec![Autd3::default(), Autd3::default()]));
    let modulation = vec![0xFF, 0xFF];

    let result = emulator.record(async move |r| {
        r.send(Modulation::new(SamplingConfig::FREQ_4K, &modulation))
            .await?;
        r.send(SetSilencer::new(FixedCompletionTime {
            intensity: 20 * ULTRASOUND_PERIOD,
            phase: 40 * ULTRASOUND_PERIOD,
            strict_mode: true,
        }))
        .await?;
        Ok(())
    });

    match result {
        Err(EmulatorError::Autd3(Error::DeviceError { device, code })) => {
            assert_eq!(device, 0);
            assert_eq!(code, DeviceErrorCode::InvalidSilencerSetting.as_u8());
        }
        Err(e) => panic!("unexpected error: {e}"),
        Ok(_) => panic!("the strict silencer was accepted"),
    }
}
