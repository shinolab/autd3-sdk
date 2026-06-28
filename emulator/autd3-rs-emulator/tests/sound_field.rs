use std::time::Duration;

use autd3_rs::common::ULTRASOUND_PERIOD;
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::operation::{FixedCompletionTime, SetSilencer};
use autd3_rs::params::NUM_TRANSDUCERS;
use autd3_rs::value::{Emission, Intensity, Phase, SamplingConfig};
use autd3_rs::{Modulation, Pattern};

use autd3_rs_emulator::{
    ClientApi, Emulator, InstantRecordOption, RangeXY, Record, RmsRecordOption,
};

fn recorded() -> Record {
    let emulator = Emulator::new(Geometry::new(vec![Autd3::default()]));
    let emissions = vec![
        [Emission {
            phase: Phase::ZERO,
            intensity: Intensity::MAX,
        }; NUM_TRANSDUCERS],
    ];
    let modulation = vec![0xFF, 0xFF];

    emulator
        .record(async move |r| {
            let mut builder = r.datagram_builder();
            builder
                .push(SetSilencer {
                    config: FixedCompletionTime {
                        intensity: ULTRASOUND_PERIOD,
                        phase: ULTRASOUND_PERIOD,
                        strict_mode: false,
                    },
                })
                .push(Modulation::new(SamplingConfig::FREQ_4K, &modulation))
                .push(Pattern::new(&emissions));
            let datagrams = builder.build()?;
            for frame in &datagrams {
                r.send_checked(frame).await?;
            }
            r.tick(2 * ULTRASOUND_PERIOD)?;
            Ok(())
        })
        .unwrap()
}

fn range() -> RangeXY {
    RangeXY {
        x: -10.0..=10.0,
        y: -10.0..=10.0,
        z: 150.0,
        resolution: 10.0,
    }
}

#[test]
fn rms_field_shape() {
    let record = recorded();
    let mut rms = record
        .sound_field(range(), RmsRecordOption::default())
        .unwrap();
    assert_eq!(rms.observe_points().height(), 9);
    let field = rms.next(ULTRASOUND_PERIOD).unwrap();
    assert_eq!(field.height(), 9);
    assert_eq!(field.width(), 1);
}

#[test]
fn instant_field_shape() {
    let record = recorded();
    let option = InstantRecordOption {
        time_step: Duration::from_micros(5),
        ..Default::default()
    };
    let mut instant = record.sound_field(range(), option).unwrap();
    assert_eq!(instant.observe_points().height(), 9);
    let field = instant.next(ULTRASOUND_PERIOD).unwrap();
    assert_eq!(field.height(), 9);
    assert_eq!(field.width(), 5);
}

#[test]
fn rms_skip_then_exhaust_errors() {
    let record = recorded();
    let mut rms = record
        .sound_field(range(), RmsRecordOption::default())
        .unwrap();
    rms.skip(ULTRASOUND_PERIOD).unwrap();
    rms.next(ULTRASOUND_PERIOD).unwrap();
    assert!(rms.next(ULTRASOUND_PERIOD).is_err());
}
