# autd3-rs-core

Core types shared across the [AUTD3](https://hapislab.org/en/airborne-ultrasound-tactile-display) sdk crates: geometry, units, values, the `Link` abstraction, and the realtime tuning helpers.

Application code normally depends on [`autd3-rs`](https://crates.io/crates/autd3-rs), which re-exports what it needs from this crate. Depend on `autd3-rs-core` directly only when writing a `Link` implementation or another sdk crate.

## Documentation

* [日本語](https://shinolab.github.io/autd3-sdk/)
* [English](https://shinolab.github.io/autd3-sdk/en/)

## License

MIT. See [LICENSE](./LICENSE).
