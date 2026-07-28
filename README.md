<h1 align="center">
AUTD3 SDK
</h1>

<p align="center">
Airborne Ultrasound Tactile Display (AUTD) is a midair haptic device that remotely produces tactile sensations on human skin without the user wearing any device.
Please see <a href="https://hapislab.org/en/airborne-ultrasound-tactile-display">our laboratory homepage</a> for more details on AUTD.
This repository contains the client libraries, firmware, and tools to drive AUTD version 3 (AUTD3) devices.
This cross-platform SDK supports Windows, macOS, and Linux (including single-board computers such as the Raspberry Pi).
</p>

## Documents

* [日本語/Japanese](https://shinolab.github.io/autd3-sdk/)
* [English](https://shinolab.github.io/autd3-sdk/en/)

## Structure

This repository is a monorepo. See the README in each subfolder for details.

- Software libraries
  - [`crates/`](./crates): Rust client library [![autd3-rs](https://img.shields.io/crates/v/autd3-rs?label=autd3-rs)](https://crates.io/crates/autd3-rs)
  - [`extras/`](./extras): Optional crates kept out of the main workspace because their GPU dependencies are slow to build
    - [`extras/autd3-rs-emulator/`](./extras/autd3-rs-emulator): Emulator [![autd3-rs-emulator](https://img.shields.io/crates/v/autd3-rs-emulator?label=autd3-rs-emulator)](https://crates.io/crates/autd3-rs-emulator)
    - [`extras/autd3-rs-pattern-holo-wgpu/`](./extras/autd3-rs-pattern-holo-wgpu): wgpu `LinAlgBackend` for `autd3-rs-pattern-holo`
  - [`bindings/ffi/`](./bindings/ffi): C API bindings
  - [`bindings/python/`](./bindings/python): Python bindings [![autd3](https://img.shields.io/pypi/v/autd3?label=autd3)](https://pypi.org/project/autd3/)
  - [`bindings/csharp/`](./bindings/csharp): C# bindings [![AUTD3](https://img.shields.io/nuget/vpre/AUTD3?label=AUTD3)](https://www.nuget.org/packages/AUTD3)
  - [`bindings/unity/`](./bindings/unity): Unity bindings [![com.shinolab.autd3-sdk](https://img.shields.io/npm/v/com.shinolab.autd3-sdk?label=com.shinolab.autd3-sdk)](https://www.npmjs.com/package/com.shinolab.autd3-sdk)
- Firmware
  - [`firmware/cpu/`](./firmware/cpu): CPU firmware
  - [`firmware/fpga/`](./firmware/fpga): FPGA firmware
- Applications
  - [`simulator/`](./simulator): Sound field simulator
  - [`console/`](./console): GUI console
  - [`tools/`](./tools): Auxiliary CLI tools
- [`examples/`](./examples): Usage examples
- [`doc/`](./doc): Documentation site sources

## Citing

If you use this SDK in your research, please consider including the following citation in your publications:

* [S. Suzuki, S. Inoue, M. Fujiwara, Y. Makino, and H. Shinoda, "AUTD3: Scalable Airborne Ultrasound Tactile Display," in IEEE Transactions on Haptics, DOI: 10.1109/TOH.2021.3069976.](https://ieeexplore.ieee.org/document/9392322)
* S. Inoue, Y. Makino and H. Shinoda "Scalable Architecture for Airborne Ultrasound Tactile Display," Asia Haptics 2016

## License

This SDK is licensed under the [MIT license](./LICENSE), except for [`crates/autd3-rs-link-soem`](./crates/autd3-rs-link-soem) and its bindings ([`bindings/ffi/autd3-ffi-link-soem`](./bindings/ffi/autd3-ffi-link-soem), [`bindings/python/autd3-link-soem`](./bindings/python/autd3-link-soem), [`bindings/csharp/src/AUTD3.Link.Soem`](./bindings/csharp/src/AUTD3.Link.Soem), and [`bindings/unity/com.shinolab.autd3-sdk.link.soem`](./bindings/unity/com.shinolab.autd3-sdk.link.soem)), which statically link [SOEM](https://github.com/OpenEtherCATsociety/SOEM) and are licensed under GPL-3.0-only.

## Author

Shun Suzuki, 2026
