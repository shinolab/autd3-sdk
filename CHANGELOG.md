# Changelog

# Rust

## [0.1.0] - 2026-07-05

### 🚀 Features

- Initial implementation of autd3-sdk
- *(twincat)* Auto-detect device count from EtherCAT master CfgSlaveCount
- *(ffi)* Add C ABI bindings for autd3-rs
- *(emulator)* Add autd3-rs-firmware-emulator crate with Audit test link
- *(pattern)* Add uniform/plane/bessel patterns with option structs
- Add Silencer command
- Add square and fourier modulation
- Add generic client-side state validation and Clear command
- Add radiation_pressure modulation processor
- Add per-device push_each command routing with Nop padding
- Add high-level STM API (foci/gain STM, circle/line)
- Fuse adjacent disjoint push_each into shared frames
- Add gain-holo crate for multi-focus hologram optimization
- Add compressed Gain STM send via WritePatternCompressed
- Add control datagrams (force fan, FPGA state read, output mask, phase correction, PWE table, GPIO)
- Decode device firmware error codes in DeviceError message
- Add Remote Link (TCP transport) and server
- Expose LoopBehavior and TransitionMode on playback commands
- *(simulator)* Add browser-based sound-field simulator tool
- Send client geometry over RemoteLink and expand the simulator
- *(csahrp)* Add full features to FFI and C#
- Add SetSilencer::disable() constructor
- *(link-soem)* Build on macOS with a custom SOEM platform layer
- Add Emission::NULL constant
- Add into_nearest() to STM config and FociStm/PatternStm
- Add Device pose accessors and Geometry::num_transducers
- Add timeout option to RemoteLink
- Add PulseWidth type and default pulse width table
- Add Nop link for examples
- *(pattern)* Make bessel theta the cone half-apex angle
- Allocate output buffers via Geometry::pattern_buffer and modulation_buffer helper instead of Client
- *(emulator)* Port hardware-free emulator with drive recording and sound field
- Derive simulator device count from client geometry
- *(twincat)* Drop TwinCATRoute for auto fallback and add *_with_timeouts constructors
- *(core)* Add now/from_utc/to_utc and Duration arithmetic to DcSysTime
- Validate transition mode against loop behavior with sys-time margin
- *(modulation)* Add constant modulation
- *(bindings)* Bring C# bindings to full parity with the library

### 🐛 Bug Fixes

- *(soem)* Return an error when DC configuration fails
- Harden client send path and structure payload errors
- *(ethercrab)* Support 3+ devices by splitting frames into groups of two
- Set static Pattern sampling divider to max to satisfy strict silencer
- Windows build (soem Packet.lib, firmware-emulator clang, ethercrab duration sub)
- Rename GpioOut::IsPatternMode to IsStmMode
- *(ffi)* Sync C ABI with autd3-rs response/checker API and add pattern buffer accessors

# Python

### 🚀 Features

- *(python)* Add PyO3 bindings for autd3-rs
- *(python)* Bring bindings to parity with the library (holo, STM, commands, remote/twincat links)
- Add SetSilencer::disable() constructor
- Add Device pose accessors and Geometry::num_transducers
- Add timeout option to RemoteLink
- Add PulseWidth type and default pulse width table
- Add Nop link for examples
- Allocate output buffers via Geometry::pattern_buffer and modulation_buffer helper instead of Client
- *(twincat)* Drop TwinCATRoute for auto fallback and add *_with_timeouts constructors
- *(core)* Add now/from_utc/to_utc and Duration arithmetic to DcSysTime
- *(modulation)* Add constant modulation
- *(py)* Add unit-type DSL, euler-angle rotations, and full Rust API parity
- *(py)* Add sequence-protocol indexing to PatternBuffer and ModulationBuffer

### 🐛 Bug Fixes

- Rename GpioOut::IsPatternMode to IsStmMode
- *(py)* Join tokio runtime at interpreter exit to avoid SIGSEGV on CPython 3.12
- *(py)* Return Response from awaited send instead of implicit check

# C#

### 🚀 Features

- *(csharp)* Add C# bindings for autd3-rs
- *(csahrp)* Add full features to FFI and C#
- Add SetSilencer::disable() constructor
- Add Device pose accessors and Geometry::num_transducers
- Add timeout option to RemoteLink
- Add PulseWidth type and default pulse width table
- Add Nop link for examples
- Allocate output buffers via Geometry::pattern_buffer and modulation_buffer helper instead of Client
- *(modulation)* Add constant modulation
- *(bindings)* Bring C# bindings to full parity with the library

### 🐛 Bug Fixes

- Windows build (soem Packet.lib, firmware-emulator clang, ethercrab duration sub)
- Rename GpioOut::IsPatternMode to IsStmMode
- *(cs)* Fix C# binding API drift from autd3-rs
- *(cs)* Put bank first in Pattern/Modulation constructors to match Rust

# Simulator

### 🚀 Features

- Send client geometry over RemoteLink and expand the simulator
- Derive simulator device count from client geometry

# Console

### 🚀 Features

- Derive simulator device count from client geometry

# Firmware

### 🚀 Features

- Initial implementation of autd3-sdk
- *(emulator)* Add autd3-rs-firmware-emulator crate with Audit test link
- Add Silencer command
- Add generic client-side state validation and Clear command
- Add per-device push_each command routing with Nop padding
- Add compressed Gain STM send via WritePatternCompressed
- Add control datagrams (force fan, FPGA state read, output mask, phase correction, PWE table, GPIO)
- Expose LoopBehavior and TransitionMode on playback commands
- Validate transition mode against loop behavior with sys-time margin

### 🐛 Bug Fixes

- Rename GpioOut::IsPatternMode to IsStmMode
