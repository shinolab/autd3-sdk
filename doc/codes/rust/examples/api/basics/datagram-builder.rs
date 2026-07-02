use anyhow::Result;

use autd3_rs::commands::{Command, Modulation, Pattern, SetSilencer};
use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::units::{Hz, m, mm, s};
use autd3_rs::value::SamplingConfig;
use autd3_rs::{Client, ClientConfig};
use autd3_rs_link_nop::Nop;
use autd3_rs_modulation::{SineOption, modulation_buffer, sine};
use autd3_rs_pattern::{FocusOption, focus, wavelength};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default(), Autd3::default()]);
    let client = Client::open(&geometry, Nop, ClientConfig::default()).await?;

    let wavelength = wavelength(340.0 * m / s);
    let option = FocusOption::default();

    let left_target = geometry.center() + offset(-40.0 * mm, 0.0 * mm, 150.0 * mm);
    let mut left = geometry.pattern_buffer();
    focus(&geometry, left_target, wavelength, &option, &mut left);

    let right_target = geometry.center() + offset(40.0 * mm, 0.0 * mm, 150.0 * mm);
    let mut right = geometry.pattern_buffer();
    focus(&geometry, right_target, wavelength, &option, &mut right);

    let mut modulation = modulation_buffer();
    sine(150 * Hz, &SineOption::default(), &mut modulation)?;

    // ANCHOR: api
    let mut builder = client.datagram_builder();
    builder.push(SetSilencer::default());
    let frames = builder.build()?;
    for frame in &frames {
        client.send_checked(frame).await?;
    }
    // ANCHOR_END: api

    // ANCHOR: push_each
    let mut builder = client.datagram_builder();
    builder.push_each(|device| {
        Some(if device % 2 == 0 {
            Pattern::new(&left)
        } else {
            Pattern::new(&right)
        })
    });
    let frames = builder.build()?;
    // ANCHOR_END: push_each

    for frame in &frames {
        client.send_checked(frame).await?;
    }

    // ANCHOR: push_each_boxed
    let mut builder = client.datagram_builder();
    builder.push_each(|device| {
        Some(if device % 2 == 0 {
            Pattern::new(&left).boxed()
        } else {
            Modulation::new(SamplingConfig::FREQ_4K, &modulation).boxed()
        })
    });
    let frames = builder.build()?;
    // ANCHOR_END: push_each_boxed

    client.close().await?;
    Ok(())
}
