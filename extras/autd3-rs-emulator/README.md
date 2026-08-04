# autd3-rs-emulator

Hardware-free emulator for [AUTD3](https://hapislab.org/en/airborne-ultrasound-tactile-display): records what the devices would emit, then computes the resulting ultrasound sound field offline.

Use it to inspect a pattern or a modulation without a device on the desk.

## Feature flags

| Feature | Default | Description |
|---------|---------|-------------|
| `polars` | yes | Return the recorded data as [polars](https://crates.io/crates/polars) frames |
| `gpu` | no | Compute the sound field on the GPU with [wgpu](https://crates.io/crates/wgpu) |

## Documentation

* [日本語](https://shinolab.github.io/autd3-sdk/)
* [English](https://shinolab.github.io/autd3-sdk/en/)

## License

MIT. See [LICENSE](./LICENSE).
