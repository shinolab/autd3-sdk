# autd3-rs-simulator-frontend

Browser frontend for the AUTD3 Simulator.

## How to use

The backend emulates AUTD3 devices over UDP on the host (IPv6 loopback): it answers the enumeration on
**`[::1]:44336`** (change it with `--port`), runs the real CPU firmware logic on the frames a client sends and displays the sound field.
The browser UI is on a separate port (default **8081**).

The client library does not know about the simulator: a client on the same host connects with `Interface::Addr([::1]:port)`
(`Interface::Auto` does not look at the loopback). To change the number or placement of the emulated devices, send the geometry
to the control port (`[::1]:port + 1`, `[::1]:44337` by default) first with `autd3-rs-simulator-control` (`simulator/control`):

```rust
autd3_rs_simulator_control::send_geometry(44336, &geometry, Duration::from_secs(5))?;
let option = TransportOption {
    iface: Interface::Addr("[::1]:44336".parse()?),
    ..Default::default()
};
let client = Client::open(&geometry, &option)?;
```

The simulator rebuilds the emulated devices and the view to match. Until a geometry has been sent, a single AUTD3 at the origin is shown.

```bash
# 1) Start the simulator (in autd3-sdk/)
cargo xtask simulator run --open                        # UI=8081, devices on [::1]:44336

# 2) Connect a client (in another terminal; example that sends a focus)
cargo xtask rust example focus_sine -- --simulator
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
cargo xtask simulator run --http-port 9000  # serve the UI on another port
cargo xtask simulator run --port 50000      # devices on [::1]:50000, control on [::1]:50001
```

## Browser requirements

Sound-field rendering uses **WebGPU**.
Latest Chrome / Edge enable it by default. 
**Firefox only supports it experimentally, so you must set `dom.webgpu.enabled` to `true` in `about:config`.**
