use anyhow::Result;

use autd3_rs::commands::{Modulation, Pattern, SetSilencer};
use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::units::{Hz, m, mm, s};
use autd3_rs::value::{Intensity, SamplingConfig};
use autd3_rs::{Client, ClientConfig};
use autd3_rs_link_nop::Nop;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let client = Client::open(&geometry, Nop, ClientConfig::default()).await?;

    let target = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let wavelength = autd3_rs_pattern::wavelength(340.0 * m / s);

    // ANCHOR: pattern_intensity
    let mut patterns = geometry.pattern_buffer();
    autd3_rs_pattern::focus(
        &geometry,
        target,
        wavelength,
        &autd3_rs_pattern::FocusOption {
            intensity: Intensity(0x80),
            ..Default::default()
        },
        &mut patterns,
    );
    // ANCHOR_END: pattern_intensity

    // ANCHOR: modulation
    let mut modulation = autd3_rs_modulation::modulation_buffer();
    autd3_rs_modulation::sine(
        200 * Hz,
        &autd3_rs_modulation::SineOption {
            amplitude: 0xFF,
            offset: 0x80,
            sampling_config: SamplingConfig::FREQ_4K,
            ..Default::default()
        },
        &mut modulation,
    )?;
    // ANCHOR_END: modulation

    let mut builder = client.datagram_builder();
    builder
        .push(SetSilencer::default())
        .push(Pattern::new(&patterns))
        .push(Modulation::new(SamplingConfig::FREQ_4K, &modulation));
    for frame in &builder.build()? {
        client.send_checked(frame).await?;
    }

    client.stop().await?;
    client.close().await?;
    Ok(())
}
