"""autd3 client facade.

The public API is organized into submodules that mirror the Rust crate layout:
``autd3.geometry``, ``autd3.value``, ``autd3.units`` and ``autd3.commands``.
The client entry points (``Client`` etc.) live at the package root.
"""

from autd3_core import (
    Autd3Error,
    DeviceError,
    Duration,
    InvalidArgumentError,
    NetworkError,
)

from . import commands, geometry, params, units, value
from ._autd3 import (
    Client,
    DeviceReply,
    DeviceState,
    DeviceStatus,
    ExecAt,
    FirmwareVersion,
    FpgaState,
    Frame,
    Frames,
    InflightFrames,
    Interface,
    LogWriter,
    Response,
    Telemetry,
    TracingGuard,
    TransportOption,
    Version,
    init_tracing,
)

__all__ = [
    "Autd3Error",
    "Client",
    "DeviceError",
    "DeviceReply",
    "DeviceState",
    "DeviceStatus",
    "Duration",
    "ExecAt",
    "FirmwareVersion",
    "FpgaState",
    "Frame",
    "Frames",
    "InflightFrames",
    "Interface",
    "InvalidArgumentError",
    "LogWriter",
    "NetworkError",
    "Response",
    "Telemetry",
    "TracingGuard",
    "TransportOption",
    "Version",
    "commands",
    "geometry",
    "init_tracing",
    "params",
    "units",
    "value",
]
