use std::num::NonZeroU16;
use std::time::Duration;

use anyhow::Result;

use autd3_rs::commands::Modulation;
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::units::{Hz, kHz};
use autd3_rs::value::SamplingConfig;
use autd3_rs::{Client, ClientConfig};
use autd3_rs_link_nop::Nop;
use autd3_rs_modulation::{Nearest, SineOption, sine};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    // ANCHOR: api
    SamplingConfig::new(NonZeroU16::new(10).unwrap());
    SamplingConfig::new(4. * kHz);
    SamplingConfig::new(Duration::from_micros(250));
    SamplingConfig::new(Nearest(4. * kHz));
    SamplingConfig::new(Nearest(Duration::from_micros(250)));
    // ANCHOR_END: api

    let geometry = Geometry::new(vec![Autd3::default()]);
    let client = Client::open(&geometry, Nop, ClientConfig::default()).await?;

    let mut modulation = autd3_rs_modulation::modulation_buffer();
    sine(150 * Hz, &SineOption::default(), &mut modulation)?;

    // ANCHOR: modulation
    let mut builder = client.datagram_builder();
    builder.push(Modulation::new(SamplingConfig::FREQ_4K, &modulation));
    // ANCHOR_END: modulation
    let _ = builder.build()?;

    client.close().await?;
    Ok(())
}
