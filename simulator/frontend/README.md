# autd3-rs-simulator-frontend

Browser frontend for the AUTD3 Simulator.

## How to use

The backend emulates AUTD3 devices over UDP on the host (IPv6 loopback): it answers the enumeration on
**`[::1]:44336`** (change it with `--port`), runs the real CPU firmware logic on the frames a client sends and displays the sound field.
The browser UI is on a separate port (default **8081**).

A client on the same host reaches the simulator on the default port with the default `TransportOption` (`Interface::Auto`
fails when both the simulator and real devices answer). Set `Interface::Simulator(port)` to connect to nothing but the
simulator on that port:

```rust
let option = TransportOption {
    iface: Interface::Simulator(SIMULATOR_PORT),
    ..Default::default()
};
let client = Client::open(&geometry, &option, ClientConfig::default()).await?;
```

The client sends its geometry to the simulator when it opens (control port `[::1]:port + 1`, `[::1]:44337` by default), and the simulator rebuilds
the emulated devices and the view to match. Until a client has connected, a single AUTD3 at the origin is shown.

```bash
# 1) Start the simulator (in autd3-sdk/)
cargo xtask simulator run --open                        # UI=8081, devices on [::1]:44336

# 2) Connect a client (in another terminal; example that sends a focus)
cargo xtask rust example focus_sine
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
