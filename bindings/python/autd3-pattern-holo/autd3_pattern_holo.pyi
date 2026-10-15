from collections.abc import Sequence
from typing import Any, ClassVar, TypeAlias, final

import numpy as np
import numpy.typing as npt
from autd3_core import Autd3Error, Geometry, Intensity, Length
from autd3_pattern import IntensityBuffer, PhaseBuffer, TransducerMask

_Vec3: TypeAlias = Sequence[float] | npt.NDArray[np.floating[Any]]

class HoloError(Autd3Error): ...

@final
class Amplitude:
    @property
    def pascal(self) -> float: ...
    @property
    def spl(self) -> float: ...
    def __mul__(self, rhs: float) -> Amplitude: ...
    def __rmul__(self, lhs: float) -> Amplitude: ...
    def __truediv__(self, rhs: float) -> Amplitude: ...
    def __eq__(self, other: object) -> bool: ...
    def __lt__(self, other: Amplitude) -> bool: ...
    def __le__(self, other: Amplitude) -> bool: ...
    def __gt__(self, other: Amplitude) -> bool: ...
    def __ge__(self, other: Amplitude) -> bool: ...
    def __hash__(self) -> int: ...

@final
class _AmplitudeUnit:
    def __rmul__(self, lhs: float) -> Amplitude: ...

Pa: _AmplitudeUnit
kPa: _AmplitudeUnit
dB: _AmplitudeUnit

@final
class AmplitudeTarget:
    def __new__(cls, point: _Vec3, amplitude: Amplitude) -> AmplitudeTarget: ...
    @property
    def point(self) -> npt.NDArray[np.float32]: ...
    @property
    def amplitude(self) -> Amplitude: ...
    def __eq__(self, other: object) -> bool: ...

@final
class IntensityConstraint:
    Normalize: ClassVar[IntensityConstraint]
    @staticmethod
    def Multiply(value: float) -> IntensityConstraint: ...
    @staticmethod
    def Uniform(intensity: Intensity | int) -> IntensityConstraint: ...
    @staticmethod
    def Clamp(min: Intensity | int, max: Intensity | int) -> IntensityConstraint: ...
    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...

@final
class Directivity:
    Sphere: ClassVar[Directivity]
    T4010A1: ClassVar[Directivity]
    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...

@final
class NaiveOption:
    def __new__(
        cls,
        constraint: IntensityConstraint = ...,
        directivity: Directivity = ...,
        mask: TransducerMask = ...,
        parallel: bool = ...,
    ) -> NaiveOption: ...
    @property
    def constraint(self) -> IntensityConstraint: ...
    @property
    def directivity(self) -> Directivity: ...
    @property
    def mask(self) -> TransducerMask: ...
    @property
    def parallel(self) -> bool: ...
    def __eq__(self, other: object) -> bool: ...

@final
class GsOption:
    def __new__(
        cls,
        repeat: int = ...,
        constraint: IntensityConstraint = ...,
        directivity: Directivity = ...,
        mask: TransducerMask = ...,
        parallel: bool = ...,
    ) -> GsOption: ...
    @property
    def repeat(self) -> int: ...
    @property
    def constraint(self) -> IntensityConstraint: ...
    @property
    def directivity(self) -> Directivity: ...
    @property
    def mask(self) -> TransducerMask: ...
    @property
    def parallel(self) -> bool: ...
    def __eq__(self, other: object) -> bool: ...

@final
class GspatOption:
    def __new__(
        cls,
        repeat: int = ...,
        constraint: IntensityConstraint = ...,
        directivity: Directivity = ...,
        mask: TransducerMask = ...,
        parallel: bool = ...,
    ) -> GspatOption: ...
    @property
    def repeat(self) -> int: ...
    @property
    def constraint(self) -> IntensityConstraint: ...
    @property
    def directivity(self) -> Directivity: ...
    @property
    def mask(self) -> TransducerMask: ...
    @property
    def parallel(self) -> bool: ...
    def __eq__(self, other: object) -> bool: ...

@final
class GreedyOption:
    def __new__(
        cls,
        phase_quantization_levels: int = ...,
        directivity: Directivity = ...,
        mask: TransducerMask = ...,
    ) -> GreedyOption: ...
    @property
    def phase_quantization_levels(self) -> int: ...
    @property
    def directivity(self) -> Directivity: ...
    @property
    def mask(self) -> TransducerMask: ...

def naive(
    geometry: Geometry,
    foci: Sequence[AmplitudeTarget],
    wavelength: Length,
    option: NaiveOption,
    phases: PhaseBuffer,
    intensities: IntensityBuffer,
) -> None: ...
def gs(
    geometry: Geometry,
    foci: Sequence[AmplitudeTarget],
    wavelength: Length,
    option: GsOption,
    phases: PhaseBuffer,
    intensities: IntensityBuffer,
) -> None: ...
def gspat(
    geometry: Geometry,
    foci: Sequence[AmplitudeTarget],
    wavelength: Length,
    option: GspatOption,
    phases: PhaseBuffer,
    intensities: IntensityBuffer,
) -> None: ...
def greedy(
    geometry: Geometry,
    foci: Sequence[AmplitudeTarget],
    wavelength: Length,
    intensities: IntensityBuffer | Intensity | int,
    option: GreedyOption,
    dst: PhaseBuffer,
) -> None: ...
def naive_batch(
    geometry: Geometry,
    foci: Sequence[AmplitudeTarget],
    wavelength: Length,
    option: NaiveOption,
    phases: Sequence[PhaseBuffer],
    intensities: Sequence[IntensityBuffer],
) -> None: ...
def gs_batch(
    geometry: Geometry,
    foci: Sequence[AmplitudeTarget],
    wavelength: Length,
    option: GsOption,
    phases: Sequence[PhaseBuffer],
    intensities: Sequence[IntensityBuffer],
) -> None: ...
def gspat_batch(
    geometry: Geometry,
    foci: Sequence[AmplitudeTarget],
    wavelength: Length,
    option: GspatOption,
    phases: Sequence[PhaseBuffer],
    intensities: Sequence[IntensityBuffer],
) -> None: ...
