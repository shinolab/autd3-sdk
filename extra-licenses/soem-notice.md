# NOTICE — SOEM (GPL-3.0-only)

This artifact statically links the Simple Open EtherCAT Master (SOEM) C library
and is therefore distributed under the **GNU General Public License v3.0 only**.
The full license text is provided alongside this file as `COPYING`.

## SOEM copyright

    Copyright (C) 2005-2025 Speciaal Machinefabriek Ketels v.o.f.
    Copyright (C) 2005-2025 Arthur Ketels
    Copyright (C) 2009-2025 RT-Labs AB, Sweden

SOEM is dual-licensed (GPLv3 / commercial); this distribution uses it under GPLv3.

## Written offer for corresponding source (GPLv3 §6)

The complete corresponding source for the GPL-covered components of this artifact
is publicly available at:

- autd3-rs-link-soem (the Rust binding and its build glue):
  https://github.com/shinolab/autd3-sdk (tag matching this release)
- SOEM (vendored as a git submodule at the pinned revision):
  https://github.com/OpenEtherCATsociety/SOEM

The exact SOEM revision used is recorded by the `3rdparty/SOEM` submodule pointer
in the autd3-sdk repository at the release tag.
