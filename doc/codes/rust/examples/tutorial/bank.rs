use anyhow::Result;

use autd3_rs::commands::Pattern;
use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::units::{m, mm, s};
use autd3_rs::value::PatternBank;
use autd3_rs::{Client, ClientConfig};
use autd3_rs_link_nop::Nop;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let client = Client::open(&geometry, Nop, ClientConfig::default()).await?;

    let wavelength = autd3_rs_pattern::wavelength(340.0 * m / s);

    // ANCHOR: switch
    // Write focus A to bank B0 and play it.
    let target_a = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let mut pat_a = geometry.pattern_buffer();
    autd3_rs_pattern::focus(
        &geometry,
        target_a,
        wavelength,
        &autd3_rs_pattern::FocusOption::default(),
        &mut pat_a,
    );
    let mut builder = client.datagram_builder();
    builder.push(Pattern::with_bank(PatternBank::B0, &pat_a));
    for frame in &builder.build()? {
        client.send_checked(frame).await?;
    }

    // Write focus B to bank B1, which is not currently playing, then switch to B1.
    // B0 keeps playing cleanly while B1 is being written (double buffering).
    let target_b = geometry.center() + offset(0.0 * mm, 30.0 * mm, 150.0 * mm);
    let mut pat_b = geometry.pattern_buffer();
    autd3_rs_pattern::focus(
        &geometry,
        target_b,
        wavelength,
        &autd3_rs_pattern::FocusOption::default(),
        &mut pat_b,
    );
    let mut builder = client.datagram_builder();
    builder.push(Pattern::with_bank(PatternBank::B1, &pat_b));
    for frame in &builder.build()? {
        client.send_checked(frame).await?;
    }
    // ANCHOR_END: switch

    client.close().await?;
    Ok(())
}
