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
from autd3.commands import FixedCompletionTime, FixedUpdateRate, SetSilencer, StmConfig
from autd3.geometry import Autd3, Device, EulerAngles, Geometry, offset, point
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
        with pytest.raises(holo.HoloError):
            holo.greedy(geometry, foci, wavelength, holo.GreedyOption(mask=mask), phases, intensities)


def test_an_unknown_group_key_is_a_key_error() -> None:
    geometry = two_devices()
    groups = pattern.TransducerGroups(geometry, lambda device, tr: "left" if device.idx() == 0 else "right")
    with pytest.raises(KeyError):
        groups.mask("center")
    left = groups.mask("left")
    assert left.num_enabled(geometry) == Autd3.NUM_TRANSDUCERS


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
            autd3.ClientConfig(ack_timeout=loose(value))
        with pytest.raises(TypeError, match="expected a Duration"):
            autd3.TransportOption(heartbeat=loose(value))


@pytest.mark.parametrize(
    "value",
    [NanosLookalike(), datetime.timedelta(milliseconds=1)],
    ids=["as_nanos-lookalike", "timedelta"],
)
def test_sampling_periods_accept_only_a_duration(value: object) -> None:
    with pytest.raises(ValueError):
        StmConfig(loose(value))
    with pytest.raises(ValueError):
        Nearest(loose(value))
    with pytest.raises(ValueError):
        autd3.value.SamplingConfig(loose(value))


def test_a_duration_is_accepted_wherever_time_is_taken() -> None:
    period = autd3.Duration.from_millis(1)
    assert SysTime.from_nanos(0) + period == SysTime.from_nanos(1_000_000)
    assert StmConfig(period).into_sampling_config(4) == autd3.value.SamplingConfig(autd3.Duration.from_micros(250))
    StmConfig(Nearest(period))
    FixedCompletionTime(intensity=period, phase=period)
    autd3.ClientConfig(ack_timeout=period)


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
    assert_float32(device.direction(0), (3,))
    assert_float32(device.positions(), (Autd3.NUM_TRANSDUCERS, 3))
    assert_float32(device.directions(), (Autd3.NUM_TRANSDUCERS, 3))
    assert_float32(device.rotation(), (4,))
    assert_float32(device.x_direction(), (3,))
    assert_float32(device.y_direction(), (3,))
    assert_float32(device.axial_direction(), (3,))
    assert_float32(device.to_local([1.0, 2.0, 3.0]), (3,))
    assert_float32(ControlPoint([1.0, 2.0, 3.0]).point, (3,))

    np.testing.assert_array_equal(device.position(5), device.positions()[5])
    np.testing.assert_array_equal(device.direction(5), device.directions()[5])


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


def device_slot() -> npt.NDArray[np.uint8]:
    return np.full(Autd3.NUM_TRANSDUCERS, 0xAA, dtype=np.uint8)


def phase_cases() -> list[
    tuple[
        str,
        Callable[[Geometry, pattern.PhaseBuffer], None],
        Callable[[Device, npt.NDArray[np.uint8]], None],
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
            lambda device, dst: pattern.focus_device(device, target, wavelength, dst),
            lambda position: pattern.focus_transducer(position, target, wavelength),
        ),
        (
            "plane",
            lambda geometry, dst: pattern.plane(geometry, direction, wavelength, dst),
            lambda device, dst: pattern.plane_device(device, direction, wavelength, dst),
            lambda position: pattern.plane_transducer(position, direction, wavelength),
        ),
        (
            "bessel",
            lambda geometry, dst: pattern.bessel(geometry, target, direction, theta, wavelength, dst),
            lambda device, dst: pattern.bessel_device(device, target, direction, theta, wavelength, dst),
            lambda position: pattern.bessel_transducer(position, target, direction, theta, wavelength),
        ),
        (
            "laguerre_gaussian_phase",
            lambda geometry, dst: pattern.laguerre_gaussian_phase(geometry, target, direction, lg, wavelength, dst),
            lambda device, dst: pattern.laguerre_gaussian_phase_device(device, target, direction, lg, wavelength, dst),
            lambda position: pattern.laguerre_gaussian_phase_transducer(position, target, direction, lg, wavelength),
        ),
        (
            "hermite_gaussian_phase",
            lambda geometry, dst: pattern.hermite_gaussian_phase(
                geometry, target, direction, x_dir, hg, wavelength, dst
            ),
            lambda device, dst: pattern.hermite_gaussian_phase_device(
                device, target, direction, x_dir, hg, wavelength, dst
            ),
            lambda position: pattern.hermite_gaussian_phase_transducer(
                position, target, direction, x_dir, hg, wavelength
            ),
        ),
    ]


@pytest.mark.parametrize("case", phase_cases(), ids=[name for name, *_ in phase_cases()])
def test_device_and_transducer_phase_functions_match_the_geometry_function(
    case: tuple[
        str,
        Callable[[Geometry, pattern.PhaseBuffer], None],
        Callable[[Device, npt.NDArray[np.uint8]], None],
        Callable[[npt.NDArray[np.float32]], Phase],
    ],
) -> None:
    _, whole, per_device, per_transducer = case
    geometry = two_devices()
    expected = geometry.phase_buffer()
    whole(geometry, expected)
    reference = expected.to_numpy()
    assert len(np.unique(reference)) > 2

    slots = np.zeros_like(reference)
    for device in geometry:
        slot = device_slot()
        per_device(device, slot)
        np.testing.assert_array_equal(slot, reference[device.idx()])
        per_device(device, slots[device.idx()])
    np.testing.assert_array_equal(slots, reference)

    rebuilt = geometry.phase_buffer()
    rebuilt.copy_from(slots)
    np.testing.assert_array_equal(rebuilt.to_numpy(), reference)

    for device in geometry:
        for tr in (0, 17, Autd3.NUM_TRANSDUCERS - 1):
            phase = per_transducer(device.position(tr))
            assert isinstance(phase, Phase)
            assert phase == expected[device.idx()][tr]


def test_device_intensity_functions_match_the_geometry_function() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0])])
    device = geometry[0]
    wavelength = pattern.wavelength(340 * m / s)
    target = [Autd3.DEVICE_WIDTH / 2, Autd3.DEVICE_HEIGHT / 2, 150.0]
    axis = [0.0, 0.0, 1.0]
    x_dir = [1.0, 0.0, 0.0]
    lg = pattern.LaguerreGaussianOption(0, 1, 30 * mm)
    hg = pattern.HermiteGaussianOption(1, 0, 30 * mm)

    expected = geometry.intensity_buffer()
    pattern.laguerre_gaussian_intensity(geometry, target, axis, lg, wavelength, expected)
    slot = device_slot()
    pattern.laguerre_gaussian_intensity_device(device, target, axis, lg, wavelength, slot)
    np.testing.assert_array_equal(slot, expected.to_numpy()[0])
    assert len(np.unique(slot)) > 2

    pattern.hermite_gaussian_intensity(geometry, target, axis, x_dir, hg, wavelength, expected)
    slot = device_slot()
    pattern.hermite_gaussian_intensity_device(device, target, axis, x_dir, hg, wavelength, slot)
    np.testing.assert_array_equal(slot, expected.to_numpy()[0])
    assert len(np.unique(slot)) > 2

    rebuilt = geometry.intensity_buffer()
    rebuilt.copy_from(slot.reshape(1, -1))
    np.testing.assert_array_equal(rebuilt.to_numpy(), expected.to_numpy())


def test_device_functions_reject_a_dst_that_is_not_one_device_slot() -> None:
    device = two_devices()[1]
    wavelength = pattern.wavelength(340 * m / s)
    target = [0.0, 0.0, 150.0]

    def write(dst: object) -> None:
        pattern.focus_device(device, target, wavelength, loose(dst))

    with pytest.raises(TypeError, match="numpy.ndarray"):
        write([0] * Autd3.NUM_TRANSDUCERS)
    with pytest.raises(TypeError, match="numpy.ndarray"):
        write(two_devices().phase_buffer()[1])
    with pytest.raises(TypeError, match="uint8"):
        write(np.zeros(Autd3.NUM_TRANSDUCERS, dtype=np.int8))
    with pytest.raises(TypeError, match="uint8"):
        write(np.zeros(Autd3.NUM_TRANSDUCERS, dtype=np.float32))
    with pytest.raises(TypeError, match="masked"):
        write(np.ma.masked_array(np.zeros(Autd3.NUM_TRANSDUCERS, dtype=np.uint8)))
    with pytest.raises(ValueError, match="shape"):
        write(np.zeros(Autd3.NUM_TRANSDUCERS - 1, dtype=np.uint8))
    with pytest.raises(ValueError, match="shape"):
        write(np.zeros((1, Autd3.NUM_TRANSDUCERS), dtype=np.uint8))
    with pytest.raises(ValueError, match="contiguous"):
        write(np.zeros(Autd3.NUM_TRANSDUCERS * 2, dtype=np.uint8)[::2])
    read_only = np.zeros(Autd3.NUM_TRANSDUCERS, dtype=np.uint8)
    read_only.flags.writeable = False
    with pytest.raises(ValueError, match="writeable"):
        write(read_only)
    with pytest.raises(ValueError):
        pattern.focus_device(loose(two_devices()), target, wavelength, device_slot())
    with pytest.raises(AttributeError):
        pattern.focus_device(loose(0), target, wavelength, device_slot())

    untouched = device_slot()
    with pytest.raises(ValueError):
        pattern.focus_device(device, loose([0.0, 0.0]), wavelength, untouched)
    np.testing.assert_array_equal(untouched, device_slot())


def test_group_device_matches_group() -> None:
    geometry = two_devices()
    groups = pattern.TransducerGroups(geometry, lambda device, tr: "low" if tr < 100 else "high")
    wavelength = pattern.wavelength(340 * m / s)
    low = geometry.phase_buffer()
    high = geometry.phase_buffer()
    pattern.focus(geometry, [0.0, 0.0, 150.0], wavelength, low)
    pattern.plane(geometry, [0.0, 0.0, 1.0], wavelength, high)
    sources = {"low": low, "high": high}
    expected = geometry.phase_buffer()
    pattern.group(geometry, groups, sources, expected)

    for device in geometry:
        slot = device_slot()
        pattern.group_device(device, groups, sources, slot)
        np.testing.assert_array_equal(slot, expected.to_numpy()[device.idx()])

    low_i = geometry.intensity_buffer()
    high_i = geometry.intensity_buffer()
    pattern.set_intensity(0x10, low_i)
    pattern.set_intensity(0x20, high_i)
    slot = device_slot()
    pattern.group_device(geometry[1], groups, {"low": low_i, "high": high_i}, slot)
    np.testing.assert_array_equal(slot, [0x10] * 100 + [0x20] * (Autd3.NUM_TRANSDUCERS - 100))

    with pytest.raises(KeyError):
        pattern.group_device(geometry[0], groups, {"low": low}, device_slot())
    with pytest.raises(TypeError):
        pattern.group_device(geometry[0], groups, loose({"low": low, "high": high_i}), device_slot())
    with pytest.raises(ValueError, match="shape"):
        pattern.group_device(geometry[0], groups, sources, np.zeros(3, dtype=np.uint8))
    single = pattern.PhaseBuffer(1)
    with pytest.raises(ValueError, match="cover the device"):
        pattern.group_device(geometry[1], groups, {"low": single, "high": single}, device_slot())


def test_fourier_mixes_exact_and_nearest_components() -> None:
    components = [
        modulation.SineComponent(100 * Hz, modulation.SineOption()),
        modulation.SineComponent(Nearest(150.0 * Hz), modulation.SineOption()),
    ]
    assert components[0].freq == 100 * Hz
    assert components[1].freq == Nearest(150.0 * Hz)
    buffer = modulation.modulation_buffer()
    modulation.fourier(components, modulation.FourierOption(), buffer)
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


@pytest.mark.parametrize("writer", ["Stdout", "Stderr"])
def test_init_tracing_writes_client_logs_and_flushes_at_exit(writer: str) -> None:
    result = run_python(
        f"""
import asyncio

import autd3
from autd3.geometry import Autd3, Geometry

_guard = autd3.init_tracing(default_filter="trace", writer=autd3.LogWriter.{writer})


async def main() -> None:
    emulator = autd3.UdpEmulator(1)
    geometry = Geometry([Autd3([0.0, 0.0, 0.0])])
    async with await autd3.Client.open(geometry, emulator.option(), autd3.ClientConfig()) as client:
        await client.send(autd3.commands.Clear())


asyncio.run(main())
"""
    )
    assert result.returncode == 0, result.stderr
    logs, other = (result.stdout, result.stderr) if writer == "Stdout" else (result.stderr, result.stdout)
    assert "autd3_rs" in logs
    assert "autd3_rs" not in other


def test_logs_keep_flowing_when_the_tracing_guard_is_discarded() -> None:
    result = run_python(
        """
import asyncio
import gc

import autd3
from autd3.geometry import Autd3, Geometry

autd3.init_tracing(default_filter="trace", writer=autd3.LogWriter.Stderr)
gc.collect()


async def main() -> None:
    emulator = autd3.UdpEmulator(1)
    geometry = Geometry([Autd3([0.0, 0.0, 0.0])])
    async with await autd3.Client.open(geometry, emulator.option(), autd3.ClientConfig()) as client:
        await client.send(autd3.commands.Clear())


asyncio.run(main())
"""
    )
    assert result.returncode == 0, result.stderr
    assert "autd3_rs" in result.stderr


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
