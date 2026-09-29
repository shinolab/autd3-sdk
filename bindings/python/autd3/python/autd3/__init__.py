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
    RtPriority,
    RtSchedulePolicy,
    DatagramBuilder,
    FpgaState,
    Frame,
    DeviceStatus,
    Frames,
    ResponseFuture,
    UdpEmulator,
    TransportOption,
)

__all__ = [
    "MAX_INFLIGHT",
    "Autd3Error",
    "Checker",
    "Client",
    "ClientConfig",
    "RtPriority",
    "RtSchedulePolicy",
    "DatagramBuilder",
    "Duration",
    "FpgaState",
    "Frame",
    "DeviceStatus",
    "Frames",
    "ResponseFuture",
    "UdpEmulator",
    "TransportOption",
    "commands",
    "geometry",
    "units",
    "value",
]
