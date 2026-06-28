use std::time::Duration;

use autd3_rs::Pattern;
use autd3_rs::common::ULTRASOUND_PERIOD;
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::operation::{FixedCompletionTime, SetSilencer};
use autd3_rs::params::NUM_TRANSDUCERS;
use autd3_rs::value::{Emission, Intensity, Phase};

use autd3_rs_emulator::{ClientApi, Emulator};

#[test]
fn records_phase_passthrough_with_silencer_disabled() {
    let emulator = Emulator::new(Geometry::new(vec![Autd3::default()]));
    let emissions = vec![
        [Emission {
            phase: Phase(0x20),
            intensity: Intensity::MAX,
        }; NUM_TRANSDUCERS],
    ];

    let record = emulator
        .record(async move |r| {
            let mut builder = r.datagram_builder();
            builder.push(SetSilencer {
                config: FixedCompletionTime {
                    intensity: ULTRASOUND_PERIOD,
                    phase: ULTRASOUND_PERIOD,
                    strict_mode: false,
                },
            });
            builder.push(Pattern::new(&emissions));
            let datagrams = builder.build()?;
            for frame in &datagrams {
                r.send_checked(frame).await?;
            }
            r.tick(2 * ULTRASOUND_PERIOD)?;
            Ok(())
        })
        .unwrap();

    assert_eq!(record.num_transducers(), NUM_TRANSDUCERS);
    assert_eq!(record.num_samples(), 2);
    assert_eq!(record.start().sys_time(), 0);
    assert_eq!(
        record.end().sys_time(),
        u64::try_from(2 * ULTRASOUND_PERIOD.as_nanos()).unwrap()
    );
    for tr in 0..NUM_TRANSDUCERS {
        assert_eq!(record.phase_of(tr), &[0x20, 0x20]);
    }
}

#[test]
fn transducer_table_shape() {
    let emulator = Emulator::new(Geometry::new(vec![Autd3::default(), Autd3::default()]));
    let table = emulator.transducer_table();
    assert_eq!(table.height(), 2 * NUM_TRANSDUCERS);
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
