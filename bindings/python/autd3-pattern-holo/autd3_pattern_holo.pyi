from collections.abc import Sequence
from typing import Any, ClassVar, TypeAlias, final

import numpy as np
import numpy.typing as npt
from autd3_core import Geometry, Intensity, Length
from autd3_pattern import IntensityBuffer, PhaseBuffer, TransducerMask

_Vec3: TypeAlias = Sequence[float] | npt.NDArray[np.floating[Any]]

class HoloError(Exception): ...

@final
class Amplitude:
    @property
    def pascal(self) -> float: ...
    @property
    def spl(self) -> float: ...
    def __eq__(self, other: object) -> bool: ...
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
        mask: TransducerMask | None = None,
        parallel: bool = True,
    ) -> NaiveOption: ...

@final
class GsOption:
    def __new__(
        cls,
        repeat: int = 100,
        constraint: IntensityConstraint = ...,
        directivity: Directivity = ...,
        mask: TransducerMask | None = None,
        parallel: bool = True,
    ) -> GsOption: ...

@final
class GspatOption:
    def __new__(
        cls,
        repeat: int = 100,
        constraint: IntensityConstraint = ...,
        directivity: Directivity = ...,
        mask: TransducerMask | None = None,
        parallel: bool = True,
    ) -> GspatOption: ...

@final
class GreedyOption:
    def __new__(
        cls,
        phase_quantization_levels: int = 16,
        constraint: IntensityConstraint = ...,
        directivity: Directivity = ...,
        mask: TransducerMask | None = None,
    ) -> GreedyOption: ...

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
    option: GreedyOption,
    phases: PhaseBuffer,
    intensities: IntensityBuffer,
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
