# autd3-rs-simulator-frontend

Browser frontend for the AUTD3 Simulator.

## How to use

The backend listens as a Remote Link server (default port **8080**), decodes the
frames sent by a connected client with the built-in firmware emulator, and displays
the sound field. The browser UI is on a separate port (default **8081**).

```bash
# 1) Start the simulator (in autd3-sdk/)
cargo xtask tool simulator --open          # UI=8081, Remote Link=8080

# 2) Connect a client (in another terminal; example that sends a focus)
cargo xtask example remote_client          # connects to 127.0.0.1:8080 and sends a focus
```

## Required tools

```bash
rustup target add wasm32-unknown-unknown
cargo install dioxus-cli
npm install
```

## Running

In `autd3-sdk/`:

```bash
cargo xtask tool simulator
cargo xtask tool simulator --open          # open the browser automatically after start
cargo xtask tool simulator --port 9000
```

> If you see the `Browserslist: caniuse-lite is outdated` warning, update it with
> `npx update-browserslist-db@latest` (in the frontend directory).

## Browser requirements

Sound-field rendering uses **WebGPU**.
Latest Chrome / Edge enable it by default. 
**Firefox only supports it experimentally, so you must set `dom.webgpu.enabled` to `true` in `about:config`.**
