import datetime
import subprocess
import sys
from collections.abc import Callable
from typing import Any

import numpy as np
import numpy.typing as npt
import pytest

import autd3
import autd3_modulation as modulation
import autd3_pattern as pattern
import autd3_pattern_holo as holo
from autd3.commands import FixedCompletionTime, FixedUpdateRate, SetSilencer
from autd3.geometry import Autd3, EulerAngles, Geometry, offset, point
from autd3.units import Hz, deg, m, mm, s
from autd3.value import ControlPoint, Nearest, Phase, SysTime
from autd3_pattern_holo import Pa


def loose(value: object) -> Any:
    return value


class NanosLookalike:
    def as_nanos(self) -> int:
        return 1_000_000


def two_devices() -> Geometry:
    return Geometry(
        [
            Autd3([0.0, 0.0, 0.0]),
            Autd3([Autd3.DEVICE_WIDTH, 0.0, 0.0], EulerAngles.ZYZ(0 * deg, 30 * deg, 0 * deg)),
        ]
    )


def test_fixed_update_rate_has_no_defaults() -> None:
    with pytest.raises(TypeError):
        loose(FixedUpdateRate)()
    with pytest.raises(TypeError):
        loose(FixedUpdateRate)(intensity=256)
    with pytest.raises(TypeError):
        loose(FixedUpdateRate)(phase=256)
    SetSilencer(FixedUpdateRate(256, 256))
    SetSilencer(FixedUpdateRate(intensity=1, phase=2))
    SetSilencer(FixedCompletionTime())


def test_a_masked_transducer_mask_is_validated_on_use_not_on_construction() -> None:
    geometry = two_devices()
    short_row = pattern.TransducerMask.masked([[True] * 3, [True] * Autd3.NUM_TRANSDUCERS])
    with pytest.raises(autd3.Autd3Error, match="transducers"):
        short_row.validate(geometry)
    one_device = pattern.TransducerMask.masked([[True] * Autd3.NUM_TRANSDUCERS])
    with pytest.raises(autd3.Autd3Error, match="device slots"):
        one_device.validate(geometry)

    foci = [holo.AmplitudeTarget(point=geometry.center() + offset(0 * mm, 0 * mm, 150 * mm), amplitude=5e3 * Pa)]
    wavelength = pattern.wavelength(340 * m / s)
    for mask in (short_row, one_device):
        phases = geometry.phase_buffer()
        intensities = geometry.intensity_buffer()
        with pytest.raises(holo.HoloError):
            holo.naive(geometry, foci, wavelength, holo.NaiveOption(mask=mask), phases, intensities)
        with pytest.raises(holo.HoloError):
            holo.gs(geometry, foci, wavelength, holo.GsOption(mask=mask), phases, intensities)
        with pytest.raises(holo.HoloError):
            holo.gspat(geometry, foci, wavelength, holo.GspatOption(mask=mask), phases, intensities)


@pytest.mark.parametrize(
    "value",
    [NanosLookalike(), datetime.timedelta(milliseconds=1), 1_000_000, 0.001, "1ms", None],
    ids=["as_nanos-lookalike", "timedelta", "int", "float", "str", "none"],
)
def test_time_arguments_accept_only_a_duration(value: object) -> None:
    with pytest.raises(TypeError, match="expected a Duration"):
        SysTime.from_nanos(0) + loose(value)
    if value is not None:
        with pytest.raises(TypeError, match="expected a Duration"):
            FixedCompletionTime(intensity=loose(value), phase=autd3.Duration.from_micros(1000))
        with pytest.raises(TypeError, match="expected a Duration"):
            autd3.TransportOption(ack_timeout=loose(value))
        with pytest.raises(TypeError, match="expected a Duration"):
            autd3.commands.CpuConfig(failsafe_timeout=loose(value))


@pytest.mark.parametrize(
    "value",
    [NanosLookalike(), datetime.timedelta(milliseconds=1)],
    ids=["as_nanos-lookalike", "timedelta"],
)
def test_sampling_periods_accept_only_a_duration(value: object) -> None:
    with pytest.raises(ValueError):
        autd3.value.SamplingConfig(loose(value))


def test_a_duration_is_accepted_wherever_time_is_taken() -> None:
    period = autd3.Duration.from_millis(1)
    assert SysTime.from_nanos(0) + period == SysTime.from_nanos(1_000_000)
    assert autd3.value.SamplingConfig(period) == autd3.value.SamplingConfig(40)
    FixedCompletionTime(intensity=period, phase=period)
    autd3.TransportOption(ack_timeout=period)


def assert_float32(array: npt.NDArray[Any], shape: tuple[int, ...]) -> None:
    assert isinstance(array, np.ndarray)
    assert array.dtype == np.float32
    assert array.shape == shape


def test_coordinates_and_rotations_are_float32() -> None:
    autd = Autd3([1.0, 2.0, 3.0], [np.cos(np.pi / 8), 0.0, 0.0, np.sin(np.pi / 8)])
    assert_float32(autd.origin, (3,))
    assert_float32(autd.rotation, (4,))
    assert_float32(point(1 * mm, 2 * mm, 3 * mm), (3,))
    assert_float32(offset(1 * mm, 2 * mm, 3 * mm), (3,))

    geometry = Geometry([autd])
    assert_float32(geometry.center(), (3,))
    device = geometry[0]
    assert_float32(device.center(), (3,))
    assert_float32(device.position(0), (3,))
    assert_float32(device[0], (3,))
    assert_float32(device.positions(), (Autd3.NUM_TRANSDUCERS, 3))
    assert_float32(device.rotation(), (4,))
    assert_float32(device.x_direction(), (3,))
    assert_float32(device.y_direction(), (3,))
    assert_float32(device.axial_direction(), (3,))
    assert_float32(device.to_local([1.0, 2.0, 3.0]), (3,))
    assert_float32(ControlPoint([1.0, 2.0, 3.0], 0).point, (3,))

    np.testing.assert_array_equal(device.position(5), device.positions()[5])


def test_autd3_rotation_is_a_scalar_first_quaternion() -> None:
    np.testing.assert_array_equal(Autd3([0.0, 0.0, 0.0]).rotation, [1.0, 0.0, 0.0, 0.0])
    quaternion = np.array([np.cos(np.pi / 8), 0.0, 0.0, np.sin(np.pi / 8)])
    autd = Autd3([0.0, 0.0, 0.0], quaternion)
    np.testing.assert_allclose(autd.rotation, quaternion, atol=1e-6)
    np.testing.assert_array_equal(Geometry([autd])[0].rotation(), autd.rotation)
    assert Autd3([0.0, 0.0, 0.0], autd.rotation) == autd
    assert Autd3(autd.origin, autd.rotation) == autd
    np.testing.assert_allclose(
        Autd3([0.0, 0.0, 0.0], EulerAngles.ZYZ(45 * deg, 0 * deg, 0 * deg)).rotation, quaternion, atol=1e-6
    )

    with pytest.raises(TypeError):
        Autd3([0.0, 0.0, 0.0], loose(None))


def test_rust_defaults_are_available_without_arguments() -> None:
    assert Phase() == Phase.ZERO
    assert autd3.value.Intensity() == autd3.value.Intensity.MAX
    geometry = Geometry([Autd3()])
    for command in (
        autd3.commands.ForceFan(),
        autd3.commands.SetFpgaConfig(),
        autd3.commands.SetGpioOut(),
        autd3.commands.EmulateGpioIn(),
    ):
        assert len(autd3.Frames.encode(geometry, command)) == 1


def phase_cases() -> list[
    tuple[
        str,
        Callable[[Geometry, pattern.PhaseBuffer], None],
        Callable[[npt.NDArray[np.float32]], Phase],
    ]
]:
    wavelength = pattern.wavelength(340 * m / s)
    target = [30.0, 40.0, 150.0]
    direction = [0.0, 0.2, 1.0]
    x_dir = [1.0, 0.0, 0.0]
    theta = 20 * deg
    lg = pattern.LaguerreGaussianOption(1, 2, 8 * mm)
    hg = pattern.HermiteGaussianOption(1, 2, 8 * mm)
    return [
        (
            "focus",
            lambda geometry, dst: pattern.focus(geometry, target, wavelength, dst),
            lambda position: pattern.focus_transducer(position, target, wavelength),
        ),
        (
            "plane",
            lambda geometry, dst: pattern.plane(geometry, direction, wavelength, dst),
            lambda position: pattern.plane_transducer(position, direction, wavelength),
        ),
        (
            "bessel",
            lambda geometry, dst: pattern.bessel(geometry, target, direction, theta, wavelength, dst),
            lambda position: pattern.bessel_transducer(position, target, direction, theta, wavelength),
        ),
        (
            "laguerre_gaussian_phase",
            lambda geometry, dst: pattern.laguerre_gaussian_phase(geometry, target, direction, lg, wavelength, dst),
            lambda position: pattern.laguerre_gaussian_phase_transducer(position, target, direction, lg, wavelength),
        ),
        (
            "hermite_gaussian_phase",
            lambda geometry, dst: pattern.hermite_gaussian_phase(
                geometry, target, direction, x_dir, hg, wavelength, dst
            ),
            lambda position: pattern.hermite_gaussian_phase_transducer(
                position, target, direction, x_dir, hg, wavelength
            ),
        ),
    ]


@pytest.mark.parametrize("case", phase_cases(), ids=[name for name, *_ in phase_cases()])
def test_transducer_phase_functions_match_the_geometry_function(
    case: tuple[
        str,
        Callable[[Geometry, pattern.PhaseBuffer], None],
        Callable[[npt.NDArray[np.float32]], Phase],
    ],
) -> None:
    _, whole, per_transducer = case
    geometry = two_devices()
    expected = geometry.phase_buffer()
    whole(geometry, expected)
    assert len(np.unique(expected.to_numpy())) > 2

    for device in geometry:
        for tr in (0, 17, Autd3.NUM_TRANSDUCERS - 1):
            phase = per_transducer(device.position(tr))
            assert isinstance(phase, Phase)
            assert phase == expected[device.idx()][tr]


def test_fourier_mixes_exact_and_nearest_components() -> None:
    components = [
        modulation.SineComponent(100 * Hz, modulation.SineOption()),
        modulation.SineComponent(Nearest(150.0 * Hz), modulation.SineOption()),
    ]
    assert components[0].freq == 100 * Hz
    assert components[1].freq == Nearest(150.0 * Hz)
    buffer = modulation.modulation_buffer()
    modulation.fourier(components, modulation.FourierOption(), autd3.value.SamplingConfig.FREQ_4K, buffer)
    assert len(buffer) > 0


def run_python(code: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run([sys.executable, "-c", code], capture_output=True, text=True, timeout=60, check=False)


def test_init_tracing_installs_once_and_reports_the_second_call() -> None:
    result = run_python(
        """
import autd3

guard = autd3.init_tracing()
assert isinstance(guard, autd3.TracingGuard)
try:
    autd3.init_tracing()
except autd3.Autd3Error as e:
    print("second:", e)
guard.close()
guard.close()
try:
    autd3.init_tracing(default_filter="debug", writer=autd3.LogWriter.Stderr)
except autd3.Autd3Error as e:
    print("after close:", e)
"""
    )
    assert result.returncode == 0, result.stderr
    assert "second: tracing is already initialized" in result.stdout
    assert "after close: tracing is already initialized" in result.stdout


def test_a_tracing_guard_is_a_context_manager() -> None:
    result = run_python(
        """
import autd3

with autd3.init_tracing(writer=autd3.LogWriter.Stderr) as guard:
    assert isinstance(guard, autd3.TracingGuard)
assert autd3.LogWriter.Stdout != autd3.LogWriter.Stderr
assert len({autd3.LogWriter.Stdout, autd3.LogWriter.Stdout}) == 1
assert repr(autd3.LogWriter.Stderr) == "LogWriter.Stderr"
"""
    )
    assert result.returncode == 0, result.stderr
