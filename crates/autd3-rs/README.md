# autd3-rs

Async client library for [AUTD3](https://hapislab.org/en/airborne-ultrasound-tactile-display), an airborne ultrasound tactile display that produces midair haptic sensations without the user wearing any device.

This crate drives AUTD3 devices over EtherCAT: it owns the realtime bus thread, builds the wire frames, and exposes an `async` API for sending emission patterns and amplitude modulation.

## Documentation

* [日本語](https://shinolab.github.io/autd3-sdk/)
* [English](https://shinolab.github.io/autd3-sdk/en/)

## Usage

`autd3-rs` provides the client; the transport, the emission patterns, and the modulation waveforms live in companion crates.

```toml
[dependencies]
autd3-rs = "0.6.0"
autd3-rs-link-echocat = "0.6.0"
autd3-rs-pattern = "0.6.0"
autd3-rs-modulation = "0.6.0"
```

```rust,no_run
use autd3_rs::commands::{Modulation, Pattern, SetSilencer};
use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::units::{Hz, m, mm, s};
use autd3_rs::value::SamplingConfig;
use autd3_rs::{Client, ClientConfig};
use autd3_rs_link_echocat::EchocatLinkOption;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let geometry = Geometry::new(vec![Autd3::default()]);
    let client = Client::open(
        &geometry,
        EchocatLinkOption::default(),
        ClientConfig::default(),
    )
    .await?;

    let target = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let wavelength = autd3_rs_pattern::wavelength(340.0 * m / s);
    let mut emissions = geometry.pattern_buffer();
    autd3_rs_pattern::focus(
        &geometry,
        target,
        wavelength,
        &autd3_rs_pattern::FocusOption::default(),
        &mut emissions,
    );

    let mut modulation = autd3_rs_modulation::modulation_buffer();
    autd3_rs_modulation::sine(
        200 * Hz,
        &autd3_rs_modulation::SineOption::default(),
        &mut modulation,
    )?;

    let mut builder = client.datagram_builder();
    builder
        .push(SetSilencer::default())
        .push(Pattern::new(&emissions))
        .push(Modulation::new(SamplingConfig::FREQ_4K, &modulation));
    for frame in &builder.build()? {
        client.send_checked(frame).await?;
    }

    client.close().await?;
    Ok(())
}
```

## Links

Pick the transport that matches your setup; all of them implement the same `Link` abstraction.

| Crate | Transport |
|-------|-----------|
| [`autd3-rs-link-echocat`](https://crates.io/crates/autd3-rs-link-echocat) | EtherCAT main device written for AUTD3 |
| [`autd3-rs-link-ethercrab`](https://crates.io/crates/autd3-rs-link-ethercrab) | EtherCAT via [EtherCrab](https://crates.io/crates/ethercrab) |
| [`autd3-rs-link-soem`](https://crates.io/crates/autd3-rs-link-soem) | EtherCAT via [SOEM](https://github.com/OpenEtherCATsociety/SOEM) (**GPL-3.0-only**) |
| [`autd3-rs-link-twincat`](https://crates.io/crates/autd3-rs-link-twincat) | TwinCAT3 via ADS (Windows) |
| [`autd3-rs-link-remote`](https://crates.io/crates/autd3-rs-link-remote) | TCP relay to a remote server driving a real link |
| [`autd3-rs-link-nop`](https://crates.io/crates/autd3-rs-link-nop) | Firmware emulator; no hardware needed |

## Feature flags

| Feature | Default | Description |
|---------|---------|-------------|
| `legacy` | no | Compatibility helpers for the pre-1.0 SDK |
| `logging` | no | `tracing-subscriber` setup helpers under `autd3_rs::rt` |

## Citing

If you use this SDK in your research, please consider including the following citation in your publications:

* [S. Suzuki, S. Inoue, M. Fujiwara, Y. Makino, and H. Shinoda, "AUTD3: Scalable Airborne Ultrasound Tactile Display," in IEEE Transactions on Haptics, DOI: 10.1109/TOH.2021.3069976.](https://ieeexplore.ieee.org/document/9392322)
* S. Inoue, Y. Makino and H. Shinoda "Scalable Architecture for Airborne Ultrasound Tactile Display," Asia Haptics 2016

## License

MIT. See [LICENSE](./LICENSE).
