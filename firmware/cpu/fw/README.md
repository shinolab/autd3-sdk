# autd3-cpu-fw

Portable logic of the [AUTD3](https://hapislab.org/en/airborne-ultrasound-tactile-display) CPU board firmware: protocol handling, command dispatch, and FPGA access.

`no_std`, no heap, and free of `unsafe` — hardware access goes through a `Port` trait, so the same sources run on the real board, in host tests, and inside [`autd3-rs-firmware-emulator`](https://crates.io/crates/autd3-rs-firmware-emulator).

## Documentation

* [日本語](https://shinolab.github.io/autd3-sdk/)
* [English](https://shinolab.github.io/autd3-sdk/en/)

## License

MIT. See [LICENSE](./LICENSE).
