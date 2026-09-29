# autd3-rs-simulator-frontend

Browser frontend for the AUTD3 Simulator.

## How to use

The backend emulates AUTD3 devices over UDP on the host (IPv6 loopback): it answers the enumeration on a group address
(default **`[::1]:44336`**), runs the real CPU firmware logic on the frames a client sends and displays the sound field.
The browser UI is on a separate port (default **8081**).

A client reaches the simulator by pointing `TransportOption.group` at the group address:

```rust
let option = TransportOption {
    group: Some("[::1]:44336".parse()?),
    ..Default::default()
};
let client = Client::open(&geometry, option, ClientConfig::default()).await?;
```

The geometry of the emulated devices is given to the simulator (`--geometry <json>`, the output of `Geometry::to_json`;
a single AUTD3 at the origin when omitted). The client's geometry must have the same number of devices.

```bash
# 1) Start the simulator (in autd3-sdk/)
cargo xtask simulator run --open                        # UI=8081, devices on [::1]:44336
cargo xtask simulator run --geometry geometry.json      # emulate the devices of a geometry file

# 2) Connect a client (in another terminal; example that sends a focus)
cargo xtask rust example focus_sine -- '[::1]:44336'
```

## Required tools

```bash
rustup target add wasm32-unknown-unknown
cargo install dioxus-cli
npm install
```

## Running

```bash
cargo xtask simulator run
cargo xtask simulator run --open          # open the browser automatically after start
cargo xtask simulator run --port 9000
cargo xtask simulator run --group '[::1]:45000'
```

## Browser requirements

Sound-field rendering uses **WebGPU**.
Latest Chrome / Edge enable it by default. 
**Firefox only supports it experimentally, so you must set `dom.webgpu.enabled` to `true` in `about:config`.**
