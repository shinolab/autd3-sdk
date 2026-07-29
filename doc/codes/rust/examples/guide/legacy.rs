use autd3_rs::commands::{ChangeModulationBank, Modulation, Pattern, SetSilencer};
use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::legacy::{LegacyChangePatternBank, LegacyClient, LegacyClientConfig};
use autd3_rs::units::{Hz, m, mm, s};
use autd3_rs::value::{ModulationBank, PatternBank, SamplingConfig, TransitionMode};
use autd3_rs_link_ethercrab::EtherCrabLinkOption;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    // ANCHOR: open
    let client = LegacyClient::open(
        &geometry,
        EtherCrabLinkOption::default(),
        LegacyClientConfig::default(),
    )
    .await?;
    // ANCHOR_END: open

    // ANCHOR: version
    for (i, version) in client.read_firmware_version().await?.iter().enumerate() {
        println!("device[{i}] firmware version: {version}");
    }
    // ANCHOR_END: version

    let target = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let mut emissions = geometry.pattern_buffer();
    autd3_rs_pattern::focus(
        &geometry,
        target,
        autd3_rs_pattern::wavelength(340.0 * m / s),
        &autd3_rs_pattern::FocusOption::default(),
        &mut emissions,
    );
    let mut modulation = autd3_rs_modulation::modulation_buffer();
    autd3_rs_modulation::sine(
        200 * Hz,
        &autd3_rs_modulation::SineOption::default(),
        &mut modulation,
    )?;

    // ANCHOR: send
    let mut builder = client.datagram_builder();
    builder
        .push(SetSilencer::default())
        .push(Pattern::new(&emissions))
        .push(Modulation::new(SamplingConfig::FREQ_4K, &modulation));
    let frames = builder.build()?;
    for frame in &frames {
        client.send_checked(frame).await?;
    }
    // ANCHOR_END: send

    // ANCHOR: change_bank
    let mut builder = client.datagram_builder();
    builder.push(Pattern::with_bank(PatternBank::B1, &emissions));
    for frame in &builder.build()? {
        client.send_checked(frame).await?;
    }

    let mut builder = client.datagram_builder();
    builder.push(LegacyChangePatternBank::pattern(PatternBank::B0));
    for frame in &builder.build()? {
        client.send_checked(frame).await?;
    }
    // ANCHOR_END: change_bank

    // ANCHOR: later
    let mut builder = client.datagram_builder();
    builder.push(Modulation {
        bank: ModulationBank::B1,
        transition_mode: TransitionMode::Later,
        ..Modulation::new(SamplingConfig::FREQ_4K, &modulation)
    });
    for frame in &builder.build()? {
        client.send_checked(frame).await?;
    }

    let mut builder = client.datagram_builder();
    builder.push(ChangeModulationBank {
        bank: ModulationBank::B1,
        transition_mode: TransitionMode::Immediate,
    });
    for frame in &builder.build()? {
        client.send_checked(frame).await?;
    }
    // ANCHOR_END: later

    // ANCHOR: close
    client.stop().await?;
    client.close().await?;
    // ANCHOR_END: close
    Ok(())
}
