# autd3-rs-simulator-frontend

Browser frontend for the AUTD3 Simulator.

## How to use

The backend emulates AUTD3 devices over UDP on the host (IPv6 loopback): it answers the enumeration on
**`[::1]:44336`** (change it with `--port`), runs the real CPU firmware logic on the frames a client sends and displays the sound field.
The browser UI is on a separate port (default **8081**).

The client library does not know about the simulator: a client on the same host connects with `Interface::Addr([::1]:port)`
(`Interface::Auto` does not look at the loopback):

```rust
let option = TransportOption {
    iface: Interface::Addr("[::1]:44336".parse()?),
    ..Default::default()
};
let client = Client::open(&geometry, &option)?;
```

The number and placement of the emulated devices are set only in the **Geometry** tab of the UI; the client does not send them.
Make them match the client's geometry before opening it (`open` fails on another number of devices).
Add (appended one device width along the last device's +x), remove and reorder the devices, and enter each origin (mm) and orientation (ZYZ Euler angles, degrees).
Device n in the tab is the n-th device of the daisy chain (`geometry[n]` of the client).
"Save to file" downloads the geometry as JSON (an array of `{ "origin": [x, y, z], "rotation": [w, x, y, z] }`) and "Load from file" loads such a file (invalid files are refused with the reason shown).
Changing the number of devices rebuilds the emulated devices (reopen the client); changing only the placement, orientation or order keeps their state (the state stays with the n-th position).
Every open browser tab shows the same geometry.
The simulator always starts with a single AUTD3 at the origin; save the geometry to a file to reuse it.

```bash
# Start the simulator (in autd3-sdk/)
cargo xtask simulator run --open                        # UI=8081, devices on [::1]:44336
```

Then open a client with `Interface::Addr([::1]:44336)`.

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
cargo xtask simulator run --port 50000      # devices on [::1]:50000
```

## Browser requirements

Sound-field rendering uses **WebGPU**.
Latest Chrome / Edge enable it by default. 
**Firefox only supports it experimentally, so you must set `dom.webgpu.enabled` to `true` in `about:config`.**
