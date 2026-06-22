# autd3-rs-link-soem

[SOEM](https://github.com/OpenEtherCATsociety/SOEM)-backed EtherCAT `Link` implementation for autd3-rs.

## ⚠️ License notice

**This crate is licensed under GPL-3.0-only, unlike the rest of the autd3-rs workspace (MIT).**

SOEM is distributed under the GNU General Public License v3, and this crate statically links it (the sources are vendored as the `3rdparty/SOEM` git submodule).
Consequently:

- If you need an MIT-licensed transport, use `autd3-rs-link-ethercrab` instead.

The full license text is in [COPYING](./COPYING).

## Building

The SOEM sources live in the `3rdparty/SOEM` submodule and are built with CMake by `build.rs`, which also generates the FFI bindings with `bindgen`:

```bash
git submodule update --init --recursive
cargo xtask rust build
```

`build.rs` requires a C/C++ toolchain (CMake) plus `libclang` for `bindgen`. On Windows install LLVM and ensure `libclang` is discoverable (e.g. set `LIBCLANG_PATH`).

`SoemLink::open` is blocking (no tokio runtime required); raw-socket access needs root / `CAP_NET_RAW` on Linux.

