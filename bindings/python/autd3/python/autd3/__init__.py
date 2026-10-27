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
    ReplyTimeoutError,
)

from . import commands, geometry, params, units, value
from ._autd3 import (
    Client,
    DeviceState,
    DeviceStatus,
    ExecAt,
    FirmwareVersion,
    FpgaState,
    Frame,
    Frames,
    Interface,
    LogWriter,
    Response,
    ResponseFuture,
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
    "DeviceState",
    "DeviceStatus",
    "Duration",
    "ExecAt",
    "FirmwareVersion",
    "FpgaState",
    "Frame",
    "Frames",
    "Interface",
    "InvalidArgumentError",
    "LogWriter",
    "NetworkError",
    "ReplyTimeoutError",
    "Response",
    "ResponseFuture",
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
