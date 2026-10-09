# autd3-rs

Async client library for [AUTD3](https://hapislab.org/en/airborne-ultrasound-tactile-display), an airborne ultrasound tactile display that produces midair haptic sensations without the user wearing any device.

This crate drives AUTD3 devices: it owns the realtime bus thread, builds the wire frames, and exposes an `async` API for sending emission patterns and amplitude modulation.

## Documentation

* [日本語](https://shinolab.github.io/autd3-sdk/)
* [English](https://shinolab.github.io/autd3-sdk/en/)

## Citing

If you use this SDK in your research, please consider including the following citation in your publications:

* [S. Suzuki, S. Inoue, M. Fujiwara, Y. Makino, and H. Shinoda, "AUTD3: Scalable Airborne Ultrasound Tactile Display," in IEEE Transactions on Haptics, DOI: 10.1109/TOH.2021.3069976.](https://ieeexplore.ieee.org/document/9392322)
* S. Inoue, Y. Makino and H. Shinoda "Scalable Architecture for Airborne Ultrasound Tactile Display," Asia Haptics 2016

## License

MIT. See [LICENSE](./LICENSE).
