# autd3 Python bindings

Python bindings for the autd3-rs SDK built with PyO3 and maturin.

| wheel | import | wraps | license |
|-------|--------|-------|---------|
| `autd3-core` | `autd3_core` | `autd3-rs-core` (geometry, value types, error) | MIT |
| `autd3` | `autd3` | `autd3-rs` (client, datagram builder, STM, commands) | MIT |
| `autd3-pattern` | `autd3_pattern` | `autd3-rs-pattern` (focus, plane, bessel, uniform, null) | MIT |
| `autd3-pattern-holo` | `autd3_pattern_holo` | `autd3-rs-pattern-holo` (naive, gs, gspat, greedy) | MIT |
| `autd3-modulation` | `autd3_modulation` | `autd3-rs-modulation` (sine, square, fourier, ...) | MIT |
| `autd3-link-echocat` | `autd3_link_echocat` | `autd3-rs-link-echocat` | MIT |
| `autd3-link-ethercrab` | `autd3_link_ethercrab` | `autd3-rs-link-ethercrab` | MIT |
| `autd3-link-remote` | `autd3_link_remote` | `autd3-rs-link-remote` (TCP transport) | MIT |
| `autd3-link-twincat` | `autd3_link_twincat` | `autd3-rs-link-twincat` (TwinCAT/ADS) | MIT |

`autd3-python-capsule` is an internal rlib holding the cross-wheel `PyCapsule` contracts and the `ClientBackend` trait.

## Build

```bash
cargo xtask py build [--debug]              # build wheels (release default)
cargo xtask py develop [--release]          # editable-install into .venv
cargo xtask py lint                          # clippy over the binding workspace
cargo xtask py format [--fix]                # rustfmt
cargo xtask py test                         # develop + pytest / import smoke
cargo xtask py example focus_sine            # develop + run examples/focus_sine.py (sudo on Unix)
```

[uv](https://docs.astral.sh/uv/) is required.
