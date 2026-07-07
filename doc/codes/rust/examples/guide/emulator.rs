use std::time::Duration;

use autd3_rs::commands::{Modulation, Pattern, SetSilencer};
use autd3_rs::common::ULTRASOUND_PERIOD;
use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::units::{m, mm, s};
use autd3_rs::value::{Emission, SamplingConfig};

use autd3_rs_emulator::{ClientApi, Emulator, InstantRecordOption, RangeXY, RmsRecordOption};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let target = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let wavelength = autd3_rs_pattern::wavelength(340.0 * m / s);
    let mut patterns =
        vec![vec![Emission::default(); Autd3::NUM_TRANSDUCERS]; geometry.num_devices()];
    autd3_rs_pattern::focus(
        &geometry,
        target,
        wavelength,
        &autd3_rs_pattern::FocusOption::default(),
        &mut patterns,
    );

    let mut modulation = Vec::new();
    autd3_rs_modulation::sine(
        200 * autd3_rs::units::Hz,
        &autd3_rs_modulation::SineOption::default(),
        &mut modulation,
    )?;

    // ANCHOR: record
    let emulator = Emulator::new(geometry);
    let record = emulator.record(async move |r| {
        let mut builder = r.datagram_builder();
        builder.push(Pattern::new(&patterns));
        let datagrams = builder.build()?;
        for frame in &datagrams {
            r.send_checked(frame).await?;
        }
        r.tick(Duration::from_millis(1))?;
        Ok(())
    })?;
    // ANCHOR_END: record

    // ANCHOR: transducer
    let table = emulator.transducer_table();
    dbg!(table);
    // ANCHOR_END: transducer

    // ANCHOR: drive
    let phase = record.phase();
    dbg!(phase);

    let pulse_width = record.pulse_width();
    dbg!(pulse_width);
    // ANCHOR_END: drive

    // ANCHOR: output
    let voltage = record.output_voltage();
    dbg!(voltage);

    let ultrasound = record.output_ultrasound();
    dbg!(ultrasound);
    // ANCHOR_END: output

    let range = RangeXY {
        x: (target.x - 20.0)..=(target.x + 20.0),
        y: (target.y - 20.0)..=(target.y + 20.0),
        z: target.z,
        resolution: 1.0,
    };
    // ANCHOR: points
    let rms = record.sound_field(range, RmsRecordOption::default())?;
    let observe_points = rms.observe_points();
    dbg!(observe_points);
    // ANCHOR_END: points

    // ANCHOR: rms
    let range = RangeXY {
        x: (target.x - 20.0)..=(target.x + 20.0),
        y: (target.y - 20.0)..=(target.y + 20.0),
        z: target.z,
        resolution: 1.0,
    };
    let mut rms = record.sound_field(range.clone(), RmsRecordOption::default())?;
    let field = rms.next(Duration::from_micros(25))?;
    dbg!(field);
    // ANCHOR_END: rms

    let range = RangeXY {
        x: (target.x - 20.0)..=(target.x + 20.0),
        y: (target.y - 20.0)..=(target.y + 20.0),
        z: target.z,
        resolution: 1.0,
    };
    // ANCHOR: instant
    let mut instant = record.sound_field(
        range,
        InstantRecordOption {
            time_step: Duration::from_micros(1),
            ..Default::default()
        },
    )?;
    instant.skip(Duration::from_micros(500))?;
    let field = instant.next(Duration::from_micros(25))?;
    dbg!(field);
    // ANCHOR_END: instant

    Ok(())
}
