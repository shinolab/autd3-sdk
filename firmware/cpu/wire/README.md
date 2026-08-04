# autd3-cpu-wire

Shared wire-protocol contract between the [AUTD3](https://hapislab.org/en/airborne-ultrasound-tactile-display) CPU firmware and its clients: command opcodes, error codes, frame layout, and payload types.

`no_std`. It is the single source of truth for both sides, so client and firmware cannot drift apart. Application code should use the re-exports from [`autd3-rs`](https://crates.io/crates/autd3-rs) rather than depending on this crate directly.

## Documentation

* [日本語](https://shinolab.github.io/autd3-sdk/)
* [English](https://shinolab.github.io/autd3-sdk/en/)

## License

MIT. See [LICENSE](./LICENSE).
