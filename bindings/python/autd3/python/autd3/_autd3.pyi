import asyncio
from collections.abc import Callable, Generator, Iterator, Sequence
from types import TracebackType
from typing import Any, ClassVar, TypeAlias, final, overload

import numpy as np
import numpy.typing as npt
from autd3_core import (
    Device,
    Duration,
    Geometry,
    Intensity,
    Length,
    Nearest,
    Freq,
    Phase,
    SamplingConfig,
    Velocity,
)
from autd3_modulation import ModulationBuffer
from autd3_pattern import IntensityBuffer, PhaseBuffer

_Vec3: TypeAlias = Sequence[float] | npt.NDArray[np.floating[Any]]
_StmConfigLike: TypeAlias = StmConfig | Freq | Duration | SamplingConfig | Nearest
_SingleCommand: TypeAlias = (
    ActivateModulationBank
    | ActivatePatternBank
    | Clear
    | ConfigFociStm
    | ConfigModulation
    | ConfigPattern
    | EmulateGpioIn
    | FociStm
    | ForceFan
    | Modulation
    | Nop
    | Pattern
    | PatternStm
    | ReleaseFailsafe
    | SetCpuConfig
    | SetGpioOut
    | SetOutputMask
    | SetPhaseCorrection
    | SetPulseWidthTable
    | SetSilencer
    | Synchronize
    | WriteFociBuffer
    | WriteModulationBuffer
    | WritePatternBuffer
    | WritePatternPhase
)
_Command: TypeAlias = _SingleCommand | Each | Sequence[_Command]

MAX_INFLIGHT: int
MAX_DEVICES: int
ULTRASOUND_PERIOD: Duration
MOD_BUFFER_SAMPLES: int
BUFFER_SIZE_MIN: int
EMISSION_MAX_INDICES: int
NUM_FOCI_MAX: int
PWE_TABLE_SIZE: int
PULSE_WIDTH_PERIOD: int
PITCH_MM: float
NUM_TRANSDUCERS: int
GRID_X: int
GRID_Y: int

@final
class DeviceState:
    Ready: ClassVar[DeviceState]
    Syncing: ClassVar[DeviceState]
    Lost: ClassVar[DeviceState]
    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...

@final
class DeviceStatus:
    @property
    def devices(self) -> list[DeviceState]: ...
    @property
    def all_ready(self) -> bool: ...
    @property
    def any_lost(self) -> bool: ...
    def __eq__(self, other: object) -> bool: ...

@final
class BusStats:
    def frames(self) -> int: ...
    def resets(self) -> int: ...
    def heartbeats(self) -> int: ...
    def missed_replies(self) -> int: ...
    def acked_frames(self) -> int: ...
    def worst_ack_latency_ns(self) -> int: ...
    def mean_ack_latency_ns(self) -> int: ...

@final
class Version:
    @property
    def major(self) -> int: ...
    @property
    def minor(self) -> int: ...
    @property
    def patch(self) -> int: ...
    def is_unknown(self) -> bool: ...
    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...

@final
class FirmwareVersion:
    SUPPORTED_SERIES: ClassVar[tuple[int, int]]
    @property
    def cpu(self) -> Version: ...
    @property
    def fpga(self) -> Version: ...
    def is_emulator(self) -> bool: ...
    def is_supported(self) -> bool: ...
    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...

@final
class FpgaState:
    def raw(self) -> int: ...
    def is_thermal_asserted(self) -> bool: ...
    def current_mod_bank(self) -> ModulationBank: ...
    def current_pattern_bank(self) -> PatternBank: ...
    def is_pattern_mode(self) -> bool: ...
    def is_stm_mode(self) -> bool: ...
    def is_pattern_stopped(self) -> bool: ...
    def is_mod_stopped(self) -> bool: ...
    def is_transition_pending(self) -> bool: ...
    def is_failsafe_active(self) -> bool: ...
    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...

@final
class Telemetry:
    FifoDrop: ClassVar[Telemetry]
    Dedup: ClassVar[Telemetry]
    SeqMismatch: ClassVar[Telemetry]
    DispatchError: ClassVar[Telemetry]
    Processed: ClassVar[Telemetry]
    Failsafe: ClassVar[Telemetry]
    SyncResync: ClassVar[Telemetry]
    PtpUnlockFailsafe: ClassVar[Telemetry]
    SendFailure: ClassVar[Telemetry]
    BootFailure: ClassVar[Telemetry]
    ALL: ClassVar[tuple[Telemetry, ...]]
    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...

@final
class TelemetryCounters:
    def get(self, counter: Telemetry) -> int: ...
    def __getitem__(self, counter: Telemetry) -> int: ...
    def as_list(self) -> list[int]: ...
    def __eq__(self, other: object) -> bool: ...

@final
class ClientConfig:
    def __new__(
        cls,
        ack_timeout: Duration | None = None,
        max_inflight: int | None = None,
        max_resync_rounds: int | None = None,
        require_supported_firmware: bool | None = None,
    ) -> ClientConfig: ...

@final
class Interface:
    Auto: ClassVar[Interface]
    Simulator: ClassVar[Interface]
    @staticmethod
    def Name(name: str) -> Interface: ...
    @staticmethod
    def Addr(addr: str) -> Interface: ...
    def name(self) -> str | None: ...
    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...

@final
class LogWriter:
    Stdout: ClassVar[LogWriter]
    Stderr: ClassVar[LogWriter]
    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...

@final
class TracingGuard:
    def close(self) -> None: ...
    def __enter__(self) -> TracingGuard: ...
    def __exit__(
        self,
        exc_type: type[BaseException] | None,
        exc: BaseException | None,
        traceback: TracebackType | None,
    ) -> None: ...

def init_tracing(default_filter: str | None = None, writer: LogWriter | None = None) -> TracingGuard: ...

@final
class TransportOption:
    def __new__(
        cls,
        iface: Interface | str | None = ...,
        heartbeat: Duration | None = ...,
        reply_timeout: Duration | None = None,
        lost_timeout: Duration | None = None,
        response_timeout: Duration | None = None,
        enumeration_timeout: Duration | None = None,
        sync_timeout: Duration | None = None,
        send_rate_limit: float | None = None,
        send_buffer: int | None = ...,
        timer_resolution: Duration | None = ...,
    ) -> TransportOption: ...
    @property
    def iface(self) -> Interface: ...
    @property
    def heartbeat(self) -> Duration | None: ...
    @property
    def reply_timeout(self) -> Duration: ...
    @property
    def lost_timeout(self) -> Duration: ...
    @property
    def response_timeout(self) -> Duration: ...
    @property
    def enumeration_timeout(self) -> Duration: ...
    @property
    def sync_timeout(self) -> Duration: ...
    @property
    def send_rate_limit(self) -> float | None: ...
    @property
    def send_buffer(self) -> int | None: ...
    @property
    def timer_resolution(self) -> Duration | None: ...

@final
class UdpEmulator:
    def __new__(cls, num_devices: int) -> UdpEmulator: ...
    @property
    def num_devices(self) -> int: ...
    def option(self) -> TransportOption: ...
    def reboot(self, index: int) -> None: ...

@final
class Response:
    @property
    def status(self) -> bytes: ...
    @property
    def values(self) -> list[bytes]: ...
    def value(self, device: int) -> bytes: ...
    def check(self) -> None: ...

@final
class ResponseFuture:
    def __await__(self) -> Generator[Any, None, Response]: ...

@final
class StreamFuture:
    def __await__(self) -> Generator[Any, None, None]: ...

@final
class StateChecker:
    def check(self) -> DeviceStatus: ...

@final
class Frame: ...

@final
class Frames:
    def __new__(cls) -> Frames: ...
    @staticmethod
    def encode(geometry: Geometry, command: _Command) -> Frames: ...
    def encode_into(self, geometry: Geometry, command: _Command) -> None: ...
    def is_empty(self) -> bool: ...
    def __len__(self) -> int: ...
    def __getitem__(self, index: int) -> Frame: ...
    def __iter__(self) -> Iterator[Frame]: ...

@final
class Client:
    @staticmethod
    def open(geometry: Geometry, option: TransportOption, config: ClientConfig) -> asyncio.Future[Client]: ...
    def num_devices(self) -> int: ...
    def state_checker(self) -> StateChecker: ...
    def bus_stats(self) -> BusStats: ...
    def geometry(self) -> Geometry: ...
    def device_time_now(self) -> SysTime: ...
    def read_firmware_version(self) -> asyncio.Future[list[FirmwareVersion]]: ...
    def read_fpga_state(self) -> asyncio.Future[list[FpgaState]]: ...
    def read_telemetry(self) -> asyncio.Future[list[TelemetryCounters]]: ...
    def send(self, command: _Command) -> asyncio.Future[None]: ...
    def send_streaming(self, command: _Command) -> asyncio.Future[StreamFuture]: ...
    def send_frame(self, frame: Frame) -> asyncio.Future[ResponseFuture]: ...
    def silent_stop(self) -> asyncio.Future[None]: ...
    def close(self) -> asyncio.Future[None]: ...
    def __aenter__(self) -> asyncio.Future[Client]: ...
    def __aexit__(
        self,
        _exc_type: type[BaseException] | None = None,
        _exc_value: BaseException | None = None,
        _traceback: TracebackType | None = None,
    ) -> asyncio.Future[None]: ...

@final
class PatternBank:
    B0: ClassVar[PatternBank]
    B1: ClassVar[PatternBank]
    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...

@final
class ModulationBank:
    B0: ClassVar[ModulationBank]
    B1: ClassVar[ModulationBank]
    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...

@final
class GpioIn:
    I0: ClassVar[GpioIn]
    I1: ClassVar[GpioIn]
    I2: ClassVar[GpioIn]
    I3: ClassVar[GpioIn]
    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...

@final
class SysTime:
    ZERO: ClassVar[SysTime]
    @staticmethod
    def from_nanos(sys_time_ns: int) -> SysTime: ...
    @property
    def sys_time(self) -> int: ...
    def __add__(self, duration: Duration) -> SysTime: ...
    @overload
    def __sub__(self, rhs: SysTime) -> Duration: ...
    @overload
    def __sub__(self, rhs: Duration) -> SysTime: ...
    def __eq__(self, other: object) -> bool: ...
    def __lt__(self, other: SysTime) -> bool: ...
    def __le__(self, other: SysTime) -> bool: ...
    def __gt__(self, other: SysTime) -> bool: ...
    def __ge__(self, other: SysTime) -> bool: ...
    def __hash__(self) -> int: ...

@final
class TransitionMode:
    SyncIdx: ClassVar[TransitionMode]
    Ext: ClassVar[TransitionMode]
    Immediate: ClassVar[TransitionMode]
    Later: ClassVar[TransitionMode]
    @staticmethod
    def SysTime(sys_time: SysTime) -> TransitionMode: ...
    @staticmethod
    def Gpio(gpio: GpioIn) -> TransitionMode: ...
    def is_later(self) -> bool: ...
    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...

@final
class LoopBehavior:
    Infinite: ClassVar[LoopBehavior]
    Once: ClassVar[LoopBehavior]
    @staticmethod
    def Finite(count: int) -> LoopBehavior: ...
    def rep(self) -> int: ...
    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...

@final
class PhaseDepth:
    Bits8: ClassVar[PhaseDepth]
    Bits4: ClassVar[PhaseDepth]
    def max_count(self) -> int: ...
    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...

@final
class PulseWidth:
    def __new__(cls, pulse_width: int) -> PulseWidth: ...
    @staticmethod
    def from_duty(duty: float) -> PulseWidth: ...
    def pulse_width(self) -> int: ...
    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...

@final
class ControlPoint:
    def __new__(cls, point: _Vec3, phase_offset: Phase | int | None = None) -> ControlPoint: ...
    @property
    def point(self) -> npt.NDArray[np.float32]: ...
    @property
    def phase_offset(self) -> Phase: ...
    def __eq__(self, other: object) -> bool: ...

@final
class ControlPoints:
    def __new__(cls, points: Sequence[ControlPoint], intensity: Intensity | int | None = None) -> ControlPoints: ...
    @property
    def points(self) -> list[ControlPoint]: ...
    @property
    def intensity(self) -> Intensity: ...
    def __eq__(self, other: object) -> bool: ...

@final
class Each: ...

def each(assign: Callable[[Device], _Command | None]) -> Each: ...

@final
class Pattern:
    def __new__(
        cls,
        phases: PhaseBuffer,
        intensities: IntensityBuffer | Intensity | int,
        bank: PatternBank | None = None,
        transition_mode: TransitionMode | None = None,
    ) -> Pattern: ...

@final
class Modulation:
    def __new__(
        cls,
        config: SamplingConfig,
        data: ModulationBuffer,
        bank: ModulationBank | None = None,
        loop_behavior: LoopBehavior | None = None,
        transition_mode: TransitionMode | None = None,
    ) -> Modulation: ...

@final
class WritePatternBuffer:
    def __new__(
        cls,
        bank: PatternBank,
        index: int,
        phases: PhaseBuffer,
        intensities: IntensityBuffer | Intensity | int,
    ) -> WritePatternBuffer: ...

@final
class WritePatternPhase:
    def __new__(
        cls,
        bank: PatternBank,
        index: int,
        depth: PhaseDepth,
        intensity: Intensity | int,
        patterns: Sequence[PhaseBuffer],
    ) -> WritePatternPhase: ...

@final
class ConfigPattern:
    def __new__(
        cls,
        bank: PatternBank,
        config: SamplingConfig,
        size: int,
        loop_behavior: LoopBehavior | None = None,
    ) -> ConfigPattern: ...

@final
class ConfigFociStm:
    def __new__(
        cls,
        bank: PatternBank,
        config: SamplingConfig,
        size: int,
        num_foci: int,
        sound_speed: Velocity,
        loop_behavior: LoopBehavior | None = None,
    ) -> ConfigFociStm: ...

@final
class ActivatePatternBank:
    def __new__(cls, bank: PatternBank, transition_mode: TransitionMode | None = None) -> ActivatePatternBank: ...

@final
class WriteModulationBuffer:
    def __new__(cls, bank: ModulationBank, offset: int, data: ModulationBuffer) -> WriteModulationBuffer: ...

@final
class ConfigModulation:
    def __new__(
        cls,
        bank: ModulationBank,
        config: SamplingConfig,
        size: int,
        loop_behavior: LoopBehavior | None = None,
    ) -> ConfigModulation: ...

@final
class ActivateModulationBank:
    def __new__(
        cls, bank: ModulationBank, transition_mode: TransitionMode | None = None
    ) -> ActivateModulationBank: ...

@final
class StmConfig:
    def __new__(cls, value: Freq | Duration | SamplingConfig | Nearest | StmConfig) -> StmConfig: ...
    def into_sampling_config(self, size: int) -> SamplingConfig: ...

@final
class FociStmOption:
    def __new__(
        cls,
        bank: PatternBank | None = None,
        sound_speed: Velocity | None = None,
        loop_behavior: LoopBehavior | None = None,
        transition_mode: TransitionMode | None = None,
    ) -> FociStmOption: ...
    @property
    def bank(self) -> PatternBank: ...
    @property
    def sound_speed(self) -> Velocity: ...
    @property
    def loop_behavior(self) -> LoopBehavior: ...
    @property
    def transition_mode(self) -> TransitionMode: ...

@final
class PatternStmOption:
    def __new__(
        cls,
        bank: PatternBank | None = None,
        phase_depth: PhaseDepth | None = None,
        loop_behavior: LoopBehavior | None = None,
        transition_mode: TransitionMode | None = None,
    ) -> PatternStmOption: ...
    @property
    def bank(self) -> PatternBank: ...
    @property
    def phase_depth(self) -> PhaseDepth: ...
    @property
    def loop_behavior(self) -> LoopBehavior: ...
    @property
    def transition_mode(self) -> TransitionMode: ...

@final
class FociStm:
    def __new__(
        cls,
        config: _StmConfigLike,
        points: Sequence[ControlPoints],
        option: FociStmOption | None = None,
    ) -> FociStm: ...

@final
class WriteFociBuffer:
    def __new__(cls, bank: PatternBank, index_offset: int, points: Sequence[ControlPoints]) -> WriteFociBuffer: ...

@final
class PatternStm:
    def __new__(
        cls,
        config: _StmConfigLike,
        phases: Sequence[PhaseBuffer],
        intensities: Intensity | int | IntensityBuffer | Sequence[IntensityBuffer],
        option: PatternStmOption | None = None,
    ) -> PatternStm: ...

def circle(
    center: _Vec3,
    radius: Length,
    num_points: int,
    normal: _Vec3,
    intensity: Intensity | int,
    dst: list[ControlPoints],
) -> None: ...
def line(
    start: _Vec3,
    end: _Vec3,
    num_points: int,
    intensity: Intensity | int,
    dst: list[ControlPoints],
) -> None: ...

@final
class Clear:
    def __new__(cls) -> Clear: ...

@final
class Synchronize:
    def __new__(cls) -> Synchronize: ...

@final
class ReleaseFailsafe:
    def __new__(cls) -> ReleaseFailsafe: ...

@final
class Nop:
    def __new__(cls) -> Nop: ...

@final
class ForceFan:
    def __new__(cls, value: bool) -> ForceFan: ...

@final
class FixedCompletionTime:
    def __new__(
        cls,
        intensity: Duration | None = None,
        phase: Duration | None = None,
        strict_mode: bool = True,
    ) -> FixedCompletionTime: ...

@final
class FixedUpdateRate:
    def __new__(cls, intensity: int, phase: int) -> FixedUpdateRate: ...

@final
class SetSilencer:
    def __new__(cls, config: FixedCompletionTime | FixedUpdateRate | None = None) -> SetSilencer: ...
    @staticmethod
    def disable() -> SetSilencer: ...

@final
class PtpConfig:
    def __new__(
        cls,
        *,
        sync_interval: Duration = ...,
        tx_timestamp_timeout: Duration = ...,
        delay_resp_timeout: Duration = ...,
        holdover: Duration = ...,
        lock_samples: int = ...,
        step_threshold: Duration = ...,
        lock_threshold: Duration = ...,
        kp_milli: int = ...,
        ki_milli: int = ...,
        max_freq_ppb: int = ...,
        delay_req_syncs: int = ...,
        path_delay_filter_shift: int = ...,
        pause_quanta: int | None = ...,
        pause_hold_syncs: int = ...,
        pause_retry: Duration = ...,
    ) -> PtpConfig: ...
    @property
    def sync_interval(self) -> Duration: ...
    @property
    def tx_timestamp_timeout(self) -> Duration: ...
    @property
    def delay_resp_timeout(self) -> Duration: ...
    @property
    def holdover(self) -> Duration: ...
    @property
    def lock_samples(self) -> int: ...
    @property
    def step_threshold(self) -> Duration: ...
    @property
    def lock_threshold(self) -> Duration: ...
    @property
    def kp_milli(self) -> int: ...
    @property
    def ki_milli(self) -> int: ...
    @property
    def max_freq_ppb(self) -> int: ...
    @property
    def delay_req_syncs(self) -> int: ...
    @property
    def path_delay_filter_shift(self) -> int: ...
    @property
    def pause_quanta(self) -> int | None: ...
    @property
    def pause_hold_syncs(self) -> int: ...
    @property
    def pause_retry(self) -> Duration: ...
    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...

@final
class FpgaBusWait:
    Cycles2: ClassVar[FpgaBusWait]
    Cycles3: ClassVar[FpgaBusWait]
    @property
    def cycles(self) -> int: ...
    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...

@final
class CpuConfig:
    def __new__(
        cls,
        *,
        sys_time_transition_margin: Duration = ...,
        fpga_wait_update_max_polls: int = ...,
        fpga_flash_max_polls: int = ...,
        sync_guard: Duration = ...,
        update_activate_delay: Duration = ...,
        failsafe_timeout: Duration | None = ...,
        ptp_unlock_failsafe_timeout: Duration | None = ...,
        fpga_bus_wait: FpgaBusWait = ...,
        ptp: PtpConfig = ...,
    ) -> CpuConfig: ...
    @property
    def sys_time_transition_margin(self) -> Duration: ...
    @property
    def fpga_wait_update_max_polls(self) -> int: ...
    @property
    def fpga_flash_max_polls(self) -> int: ...
    @property
    def sync_guard(self) -> Duration: ...
    @property
    def update_activate_delay(self) -> Duration: ...
    @property
    def failsafe_timeout(self) -> Duration | None: ...
    @property
    def ptp_unlock_failsafe_timeout(self) -> Duration | None: ...
    @property
    def fpga_bus_wait(self) -> FpgaBusWait: ...
    @property
    def ptp(self) -> PtpConfig: ...
    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...

@final
class SetCpuConfig:
    def __new__(cls, config: CpuConfig = ...) -> SetCpuConfig: ...
    @property
    def config(self) -> CpuConfig: ...

@final
class GpioOut:
    Off: ClassVar[GpioOut]
    BaseSignal: ClassVar[GpioOut]
    Thermo: ClassVar[GpioOut]
    ForceFan: ClassVar[GpioOut]
    Sync: ClassVar[GpioOut]
    ModBank: ClassVar[GpioOut]
    PatternBank: ClassVar[GpioOut]
    IsStmMode: ClassVar[GpioOut]
    SyncDiff: ClassVar[GpioOut]
    @staticmethod
    def ModIdx(idx: int) -> GpioOut: ...
    @staticmethod
    def PatternIdx(idx: int) -> GpioOut: ...
    @staticmethod
    def SysTimeEq(sys_time: SysTime) -> GpioOut: ...
    @staticmethod
    def PwmOut(transducer: int) -> GpioOut: ...
    @staticmethod
    def Direct(on: bool) -> GpioOut: ...
    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...

@final
class SetGpioOut:
    def __new__(cls, outputs: Sequence[GpioOut]) -> SetGpioOut: ...

@final
class EmulateGpioIn:
    def __new__(cls, values: Sequence[bool]) -> EmulateGpioIn: ...

@final
class SetOutputMask:
    def __new__(cls, masks: Sequence[Sequence[bool]]) -> SetOutputMask: ...

@final
class SetPhaseCorrection:
    def __new__(cls, phases: Sequence[Sequence[Phase | int]]) -> SetPhaseCorrection: ...

@final
class SetPulseWidthTable:
    def __new__(cls, table: Sequence[PulseWidth] | None = None) -> SetPulseWidthTable: ...
    @staticmethod
    def empty_table() -> list[PulseWidth]: ...
