# autd3-rs-link-soem

[SOEM](https://github.com/OpenEtherCATsociety/SOEM)-backed EtherCAT `Link` implementation for autd3-rs.

## ⚠️ License notice

**This crate is licensed under GPL-3.0-only, unlike the rest of the autd3-rs workspace (MIT).**

SOEM is distributed under the GNU General Public License v3, and this crate statically links it (the sources are vendored as the `3rdparty/SOEM` git submodule).

The full license text is in [COPYING](./COPYING).

If you need an MIT-licensed transport, use `autd3-rs-link-ethercrab` instead.

### Modified SOEM sources (macOS)

SOEM 2.x has no macOS port in its core (only an unmaintained one under `contrib/`, written against an older API). To build on macOS this crate carries a small, **modified** macOS platform layer under [`macos/`](./macos) — `osal.c` (derived from SOEM's `osal/linux/osal.c`), `nicdrv.c` / `nicdrv.h` (derived from SOEM's `contrib/oshw/macosx`), and `Darwin.cmake` (derived from SOEM's `cmake/Linux.cmake`). `build.rs` injects them into a private copy of the SOEM tree at build time; the `3rdparty/SOEM` submodule is never modified.

These files remain under SOEM's GPL-3.0 terms and carry per-file modification notices. The canonical SOEM source and license live upstream at <https://github.com/OpenEtherCATsociety/SOEM> (see its `LICENSE.md`).
