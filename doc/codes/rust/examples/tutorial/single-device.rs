use autd3_rs::commands::{Modulation, Pattern, SetSilencer};
use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::units::{Hz, m, mm, s};
use autd3_rs::value::SamplingConfig;
use autd3_rs::{Client, ClientConfig};
use autd3_rs_link_ethercrab::EtherCrabLinkOption;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Define a geometry consisting of a single AUTD3 device.
    let geometry = Geometry::new(vec![Autd3::default()]);

    // Open the client over an EtherCrab link.
    let client = Client::open(
        &geometry,
        EtherCrabLinkOption::default(),
        ClientConfig::default(),
    )
    .await?;

    // Generate a focus 150 mm above the array center.
    let target = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let wavelength = autd3_rs_pattern::wavelength(340.0 * m / s);
    let mut patterns = geometry.pattern_buffer();
    autd3_rs_pattern::focus(
        &geometry,
        target,
        wavelength,
        &autd3_rs_pattern::FocusOption::default(),
        &mut patterns,
    );

    // Apply a 200 Hz sine-wave AM.
    let mut modulation = autd3_rs_modulation::modulation_buffer();
    autd3_rs_modulation::sine(
        200 * Hz,
        &autd3_rs_modulation::SineOption {
            sampling_config: SamplingConfig::FREQ_4K,
            ..Default::default()
        },
        &mut modulation,
    )?;

    let mut builder = client.datagram_builder();
    builder
        .push(SetSilencer::default())
        .push(Pattern::new(&patterns))
        .push(Modulation::new(SamplingConfig::FREQ_4K, &modulation));
    let frames = builder.build()?;
    for frame in &frames {
        client.send_checked(frame).await?;
    }

    tokio::signal::ctrl_c().await?;

    client.stop().await?;
    client.close().await?;

    Ok(())
}
