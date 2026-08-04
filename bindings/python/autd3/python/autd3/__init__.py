"""autd3 client facade.

The public API is organized into submodules that mirror the Rust crate layout:
``autd3.geometry``, ``autd3.value``, ``autd3.units`` and ``autd3.commands``.
The client entry points (``Client`` etc.) live at the package root.
"""

from autd3_core import Autd3Error, Duration

from . import commands, geometry, units, value
from ._autd3 import (
    MAX_INFLIGHT,
    Checker,
    Client,
    ClientConfig,
    RtSchedulePolicy,
    DatagramBuilder,
    FpgaState,
    Frame,
    Frames,
    LegacyChangePatternBank,
    LegacyClient,
    LegacyClientConfig,
    LegacyDatagramBuilder,
    LegacyFrame,
    LegacyFrames,
    LinkStatus,
    ResponseFuture,
)

__all__ = [
    "MAX_INFLIGHT",
    "Autd3Error",
    "Checker",
    "Client",
    "ClientConfig",
    "RtSchedulePolicy",
    "DatagramBuilder",
    "Duration",
    "FpgaState",
    "Frame",
    "Frames",
    "LegacyChangePatternBank",
    "LegacyClient",
    "LegacyClientConfig",
    "LegacyDatagramBuilder",
    "LegacyFrame",
    "LegacyFrames",
    "LinkStatus",
    "ResponseFuture",
    "commands",
    "geometry",
    "units",
    "value",
]
