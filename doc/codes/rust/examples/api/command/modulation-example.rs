use autd3_rs::commands::Modulation;
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::units::Hz;
use autd3_rs::value::SamplingConfig;
use autd3_rs::{Client, ClientConfig};
use autd3_rs::udp::UdpOption;
use autd3_rs_modulation::{SineOption, modulation_buffer, sine};

// HIDE
#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    // HIDE_END
    let geometry = Geometry::new(vec![Autd3::default()]);
    let option = UdpOption::default();
    let client = Client::open(&geometry, &option, ClientConfig::default()).await?;

    let config = SamplingConfig::FREQ_4K;
    let mut data = modulation_buffer();
    sine(150 * Hz, &SineOption::default(), config, &mut data)?;

    client.send(Modulation::new(config, &data)).await?;

    client.close().await?;
    // HIDE
    Ok(())
}
// HIDE_END
