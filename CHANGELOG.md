# Changelog

# Rust

## [0.6.0] - 2026-08-04

### 💥 Breaking Changes

- [**breaking**] Return RemoteLinkOption from discover() in Python/C# bindings
- [**breaking**] Make wire enums non-exhaustive and rename Telemetry::COUNT
- [**breaking**] Mark the public enums non-exhaustive
- [**breaking**] Rename TransitionMode::as_u8 to try_as_u8
- [**breaking**] Give LinkError a message accessor and an error source
- [**breaking**] Encapsulate LinkStatus and CycleOutcome behind accessors
- [**breaking**] Replace the core_affinity CoreId re-export with a core newtype
- [**breaking**] Seal the Operation trait
- [**breaking**] Require a non-zero ClientConfig::timeout_cycles
- [**breaking**] Rename the holo ControlPoint to AmplitudeTarget
- [**breaking**] Replace Response::data_mut with Response::merge
- [**breaking**] Return the emission from null_transducer
- [**breaking**] Stop re-exporting the value types at the autd3-rs root
- [**breaking**] Rebuild the C ABI on opaque option handles
- [**breaking**] Follow the C ABI rework in the C# bindings
- [**breaking**] Return the reason a link opener could not be created

### 🚀 Features

- Rename `autd3-appliance list` to `scan`
- Detect firmware outside the supported series
- Re-export nalgebra from the core and holo crates

### 🐛 Bug Fixes

- *(console)* Bundle twincat-cli again by building it in CI
- *(appliance)* Tolerate bus and uplink states this client does not know
- Distinguish a rejected telemetry counter from a counter value of 2

## [0.5.0] - 2026-08-03

### 💥 Breaking Changes

- [**breaking**] Pass the holo LinAlgBackend as an argument, not an option field
- [**breaking**] Add a wgpu LinAlgBackend for holo with batched APIs and backend-side quantization
- [**breaking**] Correct SysTime drift by retiming onto the EtherCAT bus clock
- [**breaking**] Let bindings select the default, an explicit, or no RT priority
- [**breaking**] Rename legacy drive terminology to emission
- *(holo)* [**breaking**] Take Angle in Directivity::value instead of raw radians
- *(appliance)* [**breaking**] Ship the EtherCAT master as a zero-config appliance

### 🚀 Features

- *(holo)* Take batch foci as a single flat slice
- *(link-echocat)* Add echocat, an EtherCAT main device for AUTD3
- Add TransitionMode::Later to stage a bank without switching to it
- *(legacy)* Add a compatibility client for the legacy firmware v12.1.0
- Make echocat the default link and add its bindings
- *(link)* Report phase excursions and process-data exchange time through LinkStats

### 🐛 Bug Fixes

- *(client)* Return the frame slot when a send races the RT teardown
- *(legacy)* Always run the full mute sequence on close and drop
- Retime legacy SysTime onto the EtherCAT bus clock
- *(echocat)* Bound cyclic receive to one cycle and report a silent bus as lost
- Harden the legacy client and reject indivisible STM periods
- *(legacy)* Advance the emulator clock and match the firmware v12.1.0 quirks
- Harden the DC clock offset and rename dc_sys_time to bus_time_now
- Apply RT scheduling to the client and tx/rx threads by default outside Windows
- Desync the state mirror on device errors observed through the raw send path
- Write logs through a non-blocking writer in every binary

### ⚡ Performance

- Remove hypot and atan2 from holo hot paths
- *(holo)* Fuse the GSPAT iteration into a single GPU dispatch

## [0.4.0] - 2026-07-27

### 💥 Breaking Changes

- [**breaking**] Configure RT thread scheduling policy
- [**breaking**] Report firmware emulator via FirmwareVersion::is_emulator and [Emulator] display
- [**breaking**] Remove the XorHash command and fold patternsoak into perftest
- [**breaking**] Pass Device to Operation::encode and validate transducer counts per device
- [**breaking**] Pass `&Device` to `push_each` assign closure instead of device index
- [**breaking**] Make Operation single-frame and expand buffer writes as Commands
- [**breaking**] Reject modulation/STM buffers smaller than two samples
- [**breaking**] Remove ClientConfig::send_interval_cycles
- [**breaking**] Unify in_flight naming to inflight
- [**breaking**] Make ClientConfig::reset_resend_cycles NonZeroU32

### 🚀 Features

- Expose SYNC0 resync count via SyncResync telemetry
- Fuse pattern/modulation write+config+bank-change into one frame
- *(autd3-rs)* Add tracing instrumentation to send path and datagram builder

### 🐛 Bug Fixes

- Correct FOCUS_TR_X_MAX off-by-one by deriving coord/emission constants from FPGA sources
- Convert foci into device-local coordinates and send phase offsets relative to the first focus
- Check every compressed pattern group for the requested device index
- *(client)* Recover slots on link error and desync the mirror on any send failure
- Make stop() write a null pattern instead of clearing the output mask
- *(link)* Default Windows EtherCAT to a 2ms cycle with zero shift
- *(client)* Default RT thread to TimeCritical priority on Windows
- *(holo)* Account for emission intensity in greedy search

### ⚡ Performance

- Encode datagrams in place when building frames

### 📦 Dependencies

- *(deps)* Move single-crate deps out of workspace.dependencies
- *(deps)* Bump the cargo-minor group across 5 directories with 2 updates
- *(deps)* Bump cc in the cargo-minor group across 1 directory

## [0.3.0] - 2026-07-14

### 💥 Breaking Changes

- [**breaking**] Make the EtherCrab bus cycle allocation-free
- [**breaking**] Make the client send path allocation-free
- [**breaking**] Add CPU emergency stop, EtherCAT-loss failsafe and receive telemetry

### 🚀 Features

- Derive Sync0 cycle from the ESC register instead of estimating it in the FPGA
- *(csharp)* Add StmConfig.IntoSamplingConfig
- Expose pattern/mod stop and transition-pending bits in FpgaState
- *(csharp)* Add link option presets, PatternCompression.PerFrame, Device.IsEmpty and implicit StmConfig conversions

### 🐛 Bug Fixes

- *(emulator)* Keep next_sync0 ahead of the emulated sys time
- Map ERR_FPGA_TIMEOUT/ERR_SYNC_NOT_READY in the client error detail
- Use DcSysTime::now() as the EtherCAT system time source
- *(gpio)* Respect EmulateGpioIn in TransitionMode::Gpio

### ⚡ Performance

- *(ethercrab)* Stop boxing the per-device state-check future
- *(ethercrab)* Make the macOS bus cycle allocation-free

## [0.2.0] - 2026-07-09

### 💥 Breaking Changes

- [**breaking**] Scale GpioOut::SysTimeEq to FPGA time units and take DcSysTime
- [**breaking**] Read FPGA firmware version alongside CPU in read_firmware_version
- [**breaking**] Remove Autd3Unity and fix core to the right-handed/mm canonical frame

### 🚀 Features

- Decode modulation/pattern bank and STM mode from FPGA state
- *(simulator)* Add GPIO output display, MSAA, and slice rotation controls

### 🐛 Bug Fixes

- Recompute swapchain index on bank switch to avoid emulator out-of-bounds panic
- Treat ERR_UNKNOWN_CMD as unknown FPGA firmware version

## [0.1.0] - 2026-07-06

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

## [0.6.0] - 2026-08-04

### 💥 Breaking Changes

- [**breaking**] Return RemoteLinkOption from discover() in Python/C# bindings
- [**breaking**] Make wire enums non-exhaustive and rename Telemetry::COUNT
- [**breaking**] Encapsulate LinkStatus and CycleOutcome behind accessors
- [**breaking**] Replace the core_affinity CoreId re-export with a core newtype
- [**breaking**] Require a non-zero ClientConfig::timeout_cycles
- [**breaking**] Rename the holo ControlPoint to AmplitudeTarget
- [**breaking**] Follow the C ABI rework in the Python bindings
- [**breaking**] Return the reason a link opener could not be created

## [0.5.0] - 2026-08-03

### 💥 Breaking Changes

- [**breaking**] Pass the holo LinAlgBackend as an argument, not an option field
- [**breaking**] Add a wgpu LinAlgBackend for holo with batched APIs and backend-side quantization
- [**breaking**] Correct SysTime drift by retiming onto the EtherCAT bus clock
- [**breaking**] Let bindings select the default, an explicit, or no RT priority
- [**breaking**] Rename legacy drive terminology to emission
- *(emulator)* [**breaking**] Type record option sound_speed as Velocity
- *(appliance)* [**breaking**] Ship the EtherCAT master as a zero-config appliance

### 🚀 Features

- Add TransitionMode::Later to stage a bank without switching to it
- *(legacy)* Add a compatibility client for the legacy firmware v12.1.0
- Make echocat the default link and add its bindings

### 🐛 Bug Fixes

- Retime legacy SysTime onto the EtherCAT bus clock
- *(echocat)* Bound cyclic receive to one cycle and report a silent bus as lost
- Harden the legacy client and reject indivisible STM periods

## [0.4.0] - 2026-07-27

### 💥 Breaking Changes

- [**breaking**] Configure RT thread scheduling policy
- [**breaking**] Report firmware emulator via FirmwareVersion::is_emulator and [Emulator] display
- [**breaking**] Pass Device to Operation::encode and validate transducer counts per device
- [**breaking**] Pass `&Device` to `push_each` assign closure instead of device index
- [**breaking**] Reject modulation/STM buffers smaller than two samples
- [**breaking**] Remove ClientConfig::send_interval_cycles
- [**breaking**] Unify in_flight naming to inflight
- [**breaking**] Make ClientConfig::reset_resend_cycles NonZeroU32

### 📦 Dependencies

- *(deps)* Bump the cargo-minor group across 5 directories with 2 updates
- *(deps)* Bump cc in the cargo-minor group across 1 directory

## [0.3.0] - 2026-07-14

### 💥 Breaking Changes

- [**breaking**] Make the EtherCrab bus cycle allocation-free
- [**breaking**] Make the client send path allocation-free
- [**breaking**] Add CPU emergency stop, EtherCAT-loss failsafe and receive telemetry

### 🚀 Features

- Expose pattern/mod stop and transition-pending bits in FpgaState
- *(python)* Accept an Angle in Phase

### 📦 Dependencies

- *(deps)* Bump pollster from 0.4.0 to 1.0.1 in /bindings/python

## [0.2.0] - 2026-07-09

### 💥 Breaking Changes

- [**breaking**] Scale GpioOut::SysTimeEq to FPGA time units and take DcSysTime

### 🚀 Features

- *(py)* Accept EulerAngles and scipy Rotation for Autd3 rotation
- Add Python bindings for the emulator

## [0.1.0] - 2026-07-06

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

## [0.6.0] - 2026-08-04

### 💥 Breaking Changes

- [**breaking**] Return RemoteLinkOption from discover() in Python/C# bindings
- [**breaking**] Make wire enums non-exhaustive and rename Telemetry::COUNT
- [**breaking**] Rename the holo ControlPoint to AmplitudeTarget
- [**breaking**] Follow the C ABI rework in the C# bindings
- [**breaking**] Return the reason a link opener could not be created

## [0.5.0] - 2026-08-03

### 💥 Breaking Changes

- [**breaking**] Pass the holo LinAlgBackend as an argument, not an option field
- [**breaking**] Add a wgpu LinAlgBackend for holo with batched APIs and backend-side quantization
- [**breaking**] Correct SysTime drift by retiming onto the EtherCAT bus clock
- [**breaking**] Let bindings select the default, an explicit, or no RT priority
- *(appliance)* [**breaking**] Ship the EtherCAT master as a zero-config appliance

### 🚀 Features

- Add TransitionMode::Later to stage a bank without switching to it
- *(legacy)* Add a compatibility client for the legacy firmware v12.1.0
- Make echocat the default link and add its bindings

### 🐛 Bug Fixes

- Retime legacy SysTime onto the EtherCAT bus clock
- *(echocat)* Bound cyclic receive to one cycle and report a silent bus as lost
- Harden the legacy client and reject indivisible STM periods

## [0.4.0] - 2026-07-27

### 💥 Breaking Changes

- [**breaking**] Report firmware emulator via FirmwareVersion::is_emulator and [Emulator] display
- [**breaking**] Pass Device to Operation::encode and validate transducer counts per device
- [**breaking**] Pass `&Device` to `push_each` assign closure instead of device index
- [**breaking**] Remove ClientConfig::send_interval_cycles

### 🐛 Bug Fixes

- *(link)* Default Windows EtherCAT to a 2ms cycle with zero shift

## [0.3.0] - 2026-07-14

### 💥 Breaking Changes

- [**breaking**] Add CPU emergency stop, EtherCAT-loss failsafe and receive telemetry

### 🚀 Features

- *(csharp)* Add StmConfig.IntoSamplingConfig
- Expose pattern/mod stop and transition-pending bits in FpgaState
- *(csharp)* Add link option presets, PatternCompression.PerFrame, Device.IsEmpty and implicit StmConfig conversions

## [0.2.0] - 2026-07-09

### 💥 Breaking Changes

- [**breaking**] Scale GpioOut::SysTimeEq to FPGA time units and take DcSysTime

### 🚀 Features

- Add Unity bindings with per-crate UPM packages and coordinate boundary conversion

## [0.1.0] - 2026-07-06

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

# Unity

## [0.5.0] - 2026-08-03

### 🚀 Features

- Make echocat the default link and add its bindings

## [0.2.0] - 2026-07-09

### 🚀 Features

- Add Unity bindings with per-crate UPM packages and coordinate boundary conversion

## [0.1.0] - 2026-07-06

# Simulator

## [0.6.0] - 2026-08-04

### 💥 Breaking Changes

- [**breaking**] Encapsulate LinkStatus and CycleOutcome behind accessors

## [0.5.0] - 2026-08-03

### 💥 Breaking Changes

- [**breaking**] Rename legacy drive terminology to emission
- *(appliance)* [**breaking**] Ship the EtherCAT master as a zero-config appliance

### 🚀 Features

- *(console)* Distribute via cargo-dist with installers and in-app auto-update

### 🐛 Bug Fixes

- Write logs through a non-blocking writer in every binary

## [0.4.0] - 2026-07-27

### 📦 Dependencies

- *(deps)* Bump the cargo-minor group across 5 directories with 2 updates
- *(deps)* Bump cc in the cargo-minor group across 1 directory

## [0.3.0] - 2026-07-14

### 🚀 Features

- *(simulator)* Make the field canvas follow the window size
- *(simulator)* Add slice size and resolution controls

### 🐛 Bug Fixes

- Use DcSysTime::now() as the EtherCAT system time source
- *(simulator)* Keep the slice and camera when a client reconnects

## [0.2.0] - 2026-07-09

### 🚀 Features

- *(simulator)* Add GPIO output display, MSAA, and slice rotation controls

### 📦 Dependencies

- *(deps)* Bump wgpu from 29.0.3 to 30.0.0 in /simulator/frontend

## [0.1.0] - 2026-07-06

### 🚀 Features

- Send client geometry over RemoteLink and expand the simulator
- Derive simulator device count from client geometry

# Console

## [0.6.0] - 2026-08-04

### 🚀 Features

- Rename `autd3-appliance list` to `scan`

### 🐛 Bug Fixes

- *(appliance)* Tolerate bus and uplink states this client does not know

## [0.5.0] - 2026-08-03

### 💥 Breaking Changes

- *(appliance)* [**breaking**] Ship the EtherCAT master as a zero-config appliance

### 🚀 Features

- *(console)* Distribute via cargo-dist with installers and in-app auto-update

### 🐛 Bug Fixes

- *(console)* Bundle twincat-cli again by building it in CI

## [0.4.0] - 2026-07-27

### 📦 Dependencies

- *(deps)* Bump the cargo-minor group across 5 directories with 2 updates
- *(deps)* Bump cc in the cargo-minor group across 1 directory

## [0.1.0] - 2026-07-06

### 🚀 Features

- Derive simulator device count from client geometry

# Appliance

## [0.6.0] - 2026-08-04

### 🚀 Features

- Rename `autd3-appliance list` to `scan`

### 🐛 Bug Fixes

- *(appliance)* Tolerate bus and uplink states this client does not know

## [0.5.0] - 2026-08-03

### 💥 Breaking Changes

- *(appliance)* [**breaking**] Ship the EtherCAT master as a zero-config appliance

# Firmware

## [0.6.0] - 2026-08-04

### 💥 Breaking Changes

- [**breaking**] Make wire enums non-exhaustive and rename Telemetry::COUNT

## [0.4.0] - 2026-07-27

### 💥 Breaking Changes

- [**breaking**] Remove the XorHash command and fold patternsoak into perftest
- [**breaking**] Reject modulation/STM buffers smaller than two samples
- [**breaking**] Unify in_flight naming to inflight

### 🚀 Features

- Expose SYNC0 resync count via SyncResync telemetry
- Fuse pattern/modulation write+config+bank-change into one frame

### 🐛 Bug Fixes

- *(fpga)* Synchronize async THERMO input with a 2FF synchronizer
- *(fpga)* Hard-resync synchronizer on SYNC0 slip to stop permanent drift
- *(fpga)* Gate controller start-up on clk_wiz locked
- Drive unused XDCR_OUT bits low
- Use ram_style attribute for writable RAM macro
- Correct FOCUS_TR_X_MAX off-by-one by deriving coord/emission constants from FPGA sources
- Make stop() write a null pattern instead of clearing the output mask

### ⚡ Performance

- *(fpga)* Gate ec_time_to_sys_time conversion to resync window
- *(fpga)* Gate BRAM read ports with ENB to cut dynamic power
- *(fpga)* Gate ATAN and small BRAM read ports with EN
- *(fpga)* Gate swapchain_timer dividers to one division per 40kHz period
- *(fpga)* Skip the focus_calc datapath for unused foci
- *(fpga)* Drop the pwm_preconditioner snapshot stage to halve its flip-flops
- Write FPGA_STATE only when it changes

## [0.3.0] - 2026-07-14

### 💥 Breaking Changes

- [**breaking**] Add CPU emergency stop, EtherCAT-loss failsafe and receive telemetry

### 🚀 Features

- Ship prebuilt CPU platform object for standalone firmware builds
- Derive Sync0 cycle from the ESC register instead of estimating it in the FPGA
- Expose pattern/mod stop and transition-pending bits in FpgaState

### 🐛 Bug Fixes

- *(cpu)* Make the Reset FIFO flush race-free with a generation counter
- *(cpu)* Write tx frame data before ack and qualify _sTx volatile
- *(cpu)* Bound bsp/FPGA waits and fix DC reads; add ERR_FPGA_TIMEOUT/ERR_SYNC_NOT_READY
- *(cpu)* Avoid torn 64-bit DC register reads in port.c
- *(gpio)* Respect EmulateGpioIn in TransitionMode::Gpio
- *(fpga)* Spread SYS_TIME correction timing to avoid Sync0-periodic noise
- *(fpga)* Saturate sync_time_diff, guard sync-update race and timer/CDC windows

## [0.2.0] - 2026-07-09

### 💥 Breaking Changes

- [**breaking**] Read FPGA firmware version alongside CPU in read_firmware_version

### 🐛 Bug Fixes

- *(firmware)* Add memory barrier around FPGA BRAM destination-register switches

## [0.1.0] - 2026-07-06

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
