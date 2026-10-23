use std::num::NonZeroU16;
use std::time::Duration;

use anyhow::Result;

use autd3_rs::commands::Modulation;
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::units::{Hz, kHz};
use autd3_rs::value::SamplingConfig;
use autd3_rs::Client;
use autd3_rs::udp::TransportOption;
use autd3_rs_modulation::{SineOption, sine};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    // ANCHOR: api
    SamplingConfig::new(NonZeroU16::new(10).unwrap());
    SamplingConfig::from_freq(4. * kHz)?;
    SamplingConfig::from_period(Duration::from_micros(250))?;
    // ANCHOR_END: api

    let geometry = Geometry::new(vec![Autd3::default()]);
    let option = TransportOption::default();
    let client = Client::open(&geometry, &option)?;

    let mut modulation = autd3_rs_modulation::modulation_buffer();
    sine(150 * Hz, &SineOption::default(), SamplingConfig::FREQ_4K, &mut modulation)?;

    // ANCHOR: modulation
    client
        .send(Modulation::new(SamplingConfig::FREQ_4K, &modulation))
        .await?;
    // ANCHOR_END: modulation

    client.close()?;
    Ok(())
}
