"""autd3 client facade.

The public API is organized into submodules that mirror the Rust crate layout:
``autd3.geometry``, ``autd3.value``, ``autd3.units`` and ``autd3.commands``.
The client entry points (``Client`` etc.) live at the package root.
"""

from autd3_core import Autd3Error, Duration

from . import commands, geometry, params, units, value
from ._autd3 import (
    MAX_DEVICES,
    MAX_INFLIGHT,
    BusStats,
    Client,
    ClientConfig,
    DeviceState,
    DeviceStatus,
    FirmwareVersion,
    FpgaState,
    Frame,
    Frames,
    Interface,
    LogWriter,
    Response,
    ResponseFuture,
    StateChecker,
    StreamFuture,
    TelemetryCounters,
    TracingGuard,
    TransportOption,
    UdpEmulator,
    Version,
    init_tracing,
)

__all__ = [
    "MAX_DEVICES",
    "MAX_INFLIGHT",
    "Autd3Error",
    "BusStats",
    "Client",
    "ClientConfig",
    "DeviceState",
    "DeviceStatus",
    "Duration",
    "FirmwareVersion",
    "FpgaState",
    "Frame",
    "Frames",
    "Interface",
    "LogWriter",
    "Response",
    "ResponseFuture",
    "StateChecker",
    "StreamFuture",
    "TelemetryCounters",
    "TracingGuard",
    "TransportOption",
    "UdpEmulator",
    "Version",
    "commands",
    "geometry",
    "init_tracing",
    "params",
    "units",
    "value",
]
