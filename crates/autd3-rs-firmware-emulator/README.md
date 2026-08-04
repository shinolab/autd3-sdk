# autd3-rs-firmware-emulator

Software emulator of the [AUTD3](https://hapislab.org/en/airborne-ultrasound-tactile-display) device firmware.

It runs the real CPU firmware sources (`autd3-cpu-fw`) against a Rust model of the FPGA, so tests can exercise the wire protocol, the emission pipeline, and the resynchronisation logic without hardware.

Used by [`autd3-rs-link-nop`](https://crates.io/crates/autd3-rs-link-nop) and [`autd3-rs-emulator`](https://crates.io/crates/autd3-rs-emulator).

## Documentation

* [日本語](https://shinolab.github.io/autd3-sdk/)
* [English](https://shinolab.github.io/autd3-sdk/en/)

## License

MIT. See [LICENSE](./LICENSE).
