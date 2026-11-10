# autd3-rs-simulator

Sound-field simulator for AUTD3 (`autd3-simulator`), a native window (eframe + wgpu).

## How to use

The simulator emulates AUTD3 devices over UDP on the host (IPv6 loopback): it answers the enumeration on
**`[::1]:44336`** (change it with `--port`), runs the real CPU firmware logic on the frames a client sends and draws the sound field in its window.

The client library does not know about the simulator: a client on the same host connects with `Interface::Addr([::1]:port)`
(`Interface::Auto` does not look at the loopback):

```rust
let option = TransportOption {
    iface: Interface::Addr("[::1]:44336".parse()?),
    ..Default::default()
};
let client = Client::open(&geometry, &option).await?;
```

The number and placement of the emulated devices are set only in the **Geometry** tab of the window; the client does not send them.
Make them match the client's geometry before opening it (`open` fails on another number of devices).
Add (appended one device width along the last device's +x), remove and reorder the devices, and enter each origin (mm) and orientation (ZYZ Euler angles, degrees).
Device n in the tab is the n-th device of the daisy chain (`geometry[n]` of the client).
"Save to file" writes the geometry as JSON (an array of `{ "origin": [x, y, z], "rotation": [w, x, y, z] }`) and "Load from file" loads such a file (invalid files are refused with the reason shown).
Changing the number of devices rebuilds the emulated devices (reopen the client); changing only the placement, orientation or order keeps their state (the state stays with the n-th position).
The simulator always starts with a single AUTD3 at the origin; save the geometry to a file to reuse it.
The display settings (Slice / Camera / Environment / Settings tabs) are kept across restarts.

## Running

```bash
# in autd3-sdk/
cargo xtask simulator run                 # devices on [::1]:44336
cargo xtask simulator run --port 50000    # devices on [::1]:50000
```

Then open a client with `Interface::Addr([::1]:44336)`.
