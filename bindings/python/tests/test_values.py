import inspect
from collections.abc import Hashable
from typing import Any

import numpy as np
import pytest

import autd3
import autd3_modulation as modulation
import autd3_pattern as pattern
import autd3_pattern_holo as holo
from autd3.commands import CpuConfig, FpgaBusWait, GpioOut, PhaseDepth, PtpConfig
from autd3.geometry import Autd3, Geometry, offset, point
from autd3.units import Angle, Freq, Hz, Length, Velocity, deg, kHz, m, mm, rad, s
from autd3.value import (
    ControlPoint,
    ControlPoints,
    GpioIn,
    Intensity,
    LoopBehavior,
    ModulationBank,
    Nearest,
    PatternBank,
    Phase,
    SamplingConfig,
    SysTime,
    Telemetry,
    TransitionMode,
)
from autd3_pattern_holo import Pa, dB, kPa


def loose(value: object) -> Any:
    return value


EQUAL_PAIRS: list[tuple[Hashable, Hashable]] = [
    (Phase(0x10), Phase(16)),
    (Intensity(0x80), Intensity(128)),
    (SamplingConfig(4000 * Hz), SamplingConfig(10)),
    (SamplingConfig(autd3.Duration.from_micros(250)), SamplingConfig.FREQ_4K),
    (SysTime.from_nanos(5), SysTime.from_nanos(5)),
    (TransitionMode.SysTime(SysTime.from_nanos(5)), TransitionMode.SysTime(SysTime.from_nanos(5))),
    (TransitionMode.Gpio(GpioIn.I1), TransitionMode.Gpio(GpioIn.I1)),
    (TransitionMode.Later, TransitionMode.Later),
    (LoopBehavior.Once, LoopBehavior.Finite(1)),
    (LoopBehavior.Infinite, LoopBehavior.Infinite),
    (GpioIn.I2, GpioIn.I2),
    (PatternBank.B1, PatternBank.B1),
    (ModulationBank.B0, ModulationBank.B0),
    (Nearest(150.0 * Hz), Nearest(150 * Hz)),
    (Nearest(autd3.Duration.from_micros(30)), Nearest(autd3.Duration.from_nanos(30_000))),
    (autd3.Duration.from_millis(1), autd3.Duration.from_micros(1000)),
    (GpioOut.PwmOut(3), GpioOut.PwmOut(3)),
    (GpioOut.Sync, GpioOut.Sync),
    (FpgaBusWait.Cycles2, FpgaBusWait.Cycles2),
    (PtpConfig(), PtpConfig(kp_milli=PtpConfig().kp_milli)),
    (CpuConfig(), CpuConfig(ptp=PtpConfig())),
    (PhaseDepth.Bits4, PhaseDepth.Bits4),
    (Telemetry.Failsafe, Telemetry.Failsafe),
    (holo.IntensityConstraint.Normalize, holo.IntensityConstraint.Normalize),
    (holo.IntensityConstraint.Multiply(0.5), holo.IntensityConstraint.Multiply(0.5)),
    (holo.IntensityConstraint.Uniform(0x80), holo.IntensityConstraint.Uniform(Intensity(0x80))),
    (holo.IntensityConstraint.Clamp(1, 2), holo.IntensityConstraint.Clamp(1, 2)),
    (holo.Directivity.T4010A1, holo.Directivity.T4010A1),
    (2500.0 * Pa, 2.5 * kPa),
    (autd3.Interface.Name("eth0"), autd3.Interface.Name("eth0")),
    (10 * mm, Length.from_m(0.01)),
    (0.0 * mm, -0.0 * mm),
    (2 * kHz, 2000.0 * Hz),
    (340 * m / s, Velocity.from_mm_s(340_000.0)),
    (0.0 * rad, Angle.ZERO),
    (180 * deg, Angle.from_deg(180.0)),
]

DISTINCT_PAIRS: list[tuple[object, object]] = [
    (Phase(1), Phase(2)),
    (Phase(1), Intensity(1)),
    (Phase(1), 1),
    (Intensity(1), Intensity(2)),
    (SamplingConfig(10), SamplingConfig(20)),
    (SysTime.from_nanos(5), SysTime.from_nanos(6)),
    (TransitionMode.Immediate, TransitionMode.Later),
    (TransitionMode.Gpio(GpioIn.I1), TransitionMode.Gpio(GpioIn.I2)),
    (LoopBehavior.Finite(2), LoopBehavior.Finite(3)),
    (LoopBehavior.Infinite, LoopBehavior.Once),
    (GpioIn.I0, GpioIn.I1),
    (PatternBank.B0, PatternBank.B1),
    (PatternBank.B0, ModulationBank.B0),
    (Nearest(150.0 * Hz), Nearest(151.0 * Hz)),
    (autd3.Duration.from_millis(1), autd3.Duration.from_millis(2)),
    (GpioOut.PwmOut(3), GpioOut.PwmOut(4)),
    (GpioOut.ModIdx(3), GpioOut.PatternIdx(3)),
    (FpgaBusWait.Cycles2, FpgaBusWait.Cycles3),
    (PtpConfig(), PtpConfig(kp_milli=PtpConfig().kp_milli + 1)),
    (CpuConfig(), CpuConfig(failsafe_timeout=None)),
    (holo.IntensityConstraint.Multiply(0.5), holo.IntensityConstraint.Multiply(0.25)),
    (holo.IntensityConstraint.Normalize, holo.IntensityConstraint.Uniform(0)),
    (holo.Directivity.Sphere, holo.Directivity.T4010A1),
    (1.0 * Pa, 2.0 * Pa),
    (autd3.Interface.Auto, autd3.Interface.Simulator),
    (10 * mm, 11 * mm),
    (10 * mm, 10.0),
    (200 * Hz, 201 * Hz),
    (1.0 * rad, 1.0 * deg),
]


@pytest.mark.parametrize(("lhs", "rhs"), EQUAL_PAIRS)
def test_equal_values_compare_and_hash_alike(lhs: Hashable, rhs: Hashable) -> None:
    assert lhs == rhs
    assert not (lhs != rhs)
    assert hash(lhs) == hash(rhs)
    assert len({lhs, rhs}) == 1


@pytest.mark.parametrize(("lhs", "rhs"), DISTINCT_PAIRS)
def test_distinct_values_compare_unequal(lhs: object, rhs: object) -> None:
    assert lhs != rhs
    assert not (lhs == rhs)


def test_an_invalid_sampling_config_equals_nothing() -> None:
    invalid = SamplingConfig(3000 * Hz)
    with pytest.raises(autd3.Autd3Error):
        invalid.divide()
    assert invalid != invalid
    assert invalid != SamplingConfig(3000 * Hz)
    hash(invalid)


def test_value_types_are_immutable() -> None:
    with pytest.raises(AttributeError):
        loose(Phase(1)).value = 2
    with pytest.raises(AttributeError):
        loose(10 * mm).mm = 2.0


def test_sampling_config_reports_typed_period_and_frequency() -> None:
    config = SamplingConfig(10)
    assert config.period() == autd3.Duration.from_micros(250)
    assert config.period() == autd3.params.ULTRASOUND_PERIOD.__class__.from_nanos(
        10 * autd3.params.ULTRASOUND_PERIOD.as_nanos()
    )
    assert config.freq() == 4000 * Hz
    assert isinstance(config.freq(), Freq)


def test_units_have_explicit_constructors() -> None:
    assert Length.from_mm(10.0) == 10 * mm
    assert Length.from_m(1.0) == 1000 * mm
    assert Freq.from_hz(200) == 200 * Hz
    assert Freq.from_hz(200).is_int
    assert not Freq.from_hz(200.0).is_int
    assert Angle.ZERO.rad == 0.0
    with pytest.raises(ValueError):
        Freq.from_hz(-1)


def test_phase_conversions() -> None:
    assert Phase(1j) == Phase(0x40)
    assert Phase(-1 + 0j) == Phase.PI
    assert Phase(np.complex64(1j)) == Phase(0x40)
    assert Phase(np.complex128(-1j)) == Phase(0xC0)
    assert Phase(0x40).rad() == pytest.approx(np.pi / 2)
    assert [10, 20, 30][Phase(1)] == 20
    assert int(Phase(7)) == 7
    with pytest.raises(ValueError):
        Phase(loose(1.5))
    assert not hasattr(Phase(0), "radian")


def test_loop_behavior_and_transition_mode_names() -> None:
    assert LoopBehavior.Once.rep() == 0
    assert not hasattr(LoopBehavior, "ONCE")
    assert TransitionMode.Later.is_later()
    assert not TransitionMode.Immediate.is_later()
    assert not TransitionMode.Gpio(GpioIn.I0).is_later()


def test_telemetry_all_lists_every_counter_once() -> None:
    assert isinstance(Telemetry.ALL, tuple)
    assert len(set(Telemetry.ALL)) == len(Telemetry.ALL)
    assert Telemetry.FifoDrop in Telemetry.ALL
    assert Telemetry.PtpUnlockFailsafe in Telemetry.ALL
    assert Telemetry.SendFailure in Telemetry.ALL
    assert Telemetry.BootFailure in Telemetry.ALL


def test_params_mirror_the_rust_constants() -> None:
    params = autd3.params
    assert params.ULTRASOUND_PERIOD == SamplingConfig.FREQ_40K.period()
    assert params.NUM_TRANSDUCERS == Autd3.NUM_TRANSDUCERS == Geometry([Autd3([0.0, 0.0, 0.0])]).num_transducers()
    assert params.GRID_X == Autd3.GRID_X
    assert params.GRID_Y == Autd3.GRID_Y
    assert params.GRID_X * params.GRID_Y - 3 == params.NUM_TRANSDUCERS
    assert params.PITCH_MM == Autd3.PITCH_MM
    assert params.MAX_INFLIGHT == autd3.MAX_INFLIGHT
    assert params.MAX_DEVICES == autd3.MAX_DEVICES
    assert params.PWE_TABLE_SIZE == len(autd3.commands.SetPulseWidthTable.empty_table())
    assert params.BUFFER_SIZE_MIN >= 1
    assert params.NUM_FOCI_MAX >= 1
    assert params.EMISSION_MAX_INDICES >= params.BUFFER_SIZE_MIN
    assert params.MOD_BUFFER_SAMPLES >= params.BUFFER_SIZE_MIN
    assert params.PULSE_WIDTH_PERIOD > 0
    assert set(params.__all__) <= set(dir(params))


def test_geometry_helpers() -> None:
    np.testing.assert_array_equal(point(1 * mm, 2 * mm, 3 * m), [1.0, 2.0, 3000.0])
    np.testing.assert_array_equal(offset(1 * mm, 2 * mm, 3 * mm), [1.0, 2.0, 3.0])
    with pytest.raises(TypeError):
        offset(loose(1.0), 2 * mm, 3 * mm)

    geometry = Geometry([Autd3([0.0, 0.0, 0.0]), Autd3([Autd3.DEVICE_WIDTH, 0.0, 0.0])])
    assert repr(geometry) == f"Geometry(num_devices=2, num_transducers={2 * Autd3.NUM_TRANSDUCERS})"
    devices = list(geometry)
    assert [device.idx() for device in devices] == [0, 1]
    assert repr(devices[1]) == f"Device(idx=1, num_transducers={Autd3.NUM_TRANSDUCERS})"
    assert geometry == Geometry([Autd3([0.0, 0.0, 0.0]), Autd3([Autd3.DEVICE_WIDTH, 0.0, 0.0])])
    assert geometry != Geometry([Autd3([0.0, 0.0, 0.0])])
    assert geometry != Geometry([Autd3([0.0, 0.0, 0.0]), Autd3([Autd3.DEVICE_WIDTH, 0.0, 1.0])])
    assert devices[0] == geometry[0]
    assert devices[0] != geometry[1]
    np.testing.assert_array_equal(devices[1][0], devices[1].position(0))
    np.testing.assert_allclose(devices[1][1], [Autd3.DEVICE_WIDTH + Autd3.PITCH_MM, 0.0, 0.0])
    with pytest.raises(IndexError):
        devices[1][Autd3.NUM_TRANSDUCERS]
    np.testing.assert_allclose(devices[1].to_local([Autd3.DEVICE_WIDTH + 1.0, 2.0, 3.0]), [1.0, 2.0, 3.0])

    rotated = Geometry([Autd3([10.0, 0.0, 0.0], [np.cos(np.pi / 4), 0.0, 0.0, np.sin(np.pi / 4)])])[0]
    np.testing.assert_allclose(rotated.to_local([10.0, 5.0, 0.0]), [5.0, 0.0, 0.0], atol=1e-5)


def test_transducer_mask_queries() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0]), Autd3([Autd3.DEVICE_WIDTH, 0.0, 0.0])])
    everything = pattern.TransducerMask.AllEnabled
    everything.validate(geometry)
    assert everything.is_enabled(1, 3)
    assert everything.num_enabled(geometry) == geometry.num_transducers()

    rows = [[tr % 2 == 0 for tr in range(Autd3.NUM_TRANSDUCERS)] for _ in range(2)]
    mask = pattern.TransducerMask.masked(rows)
    mask.validate(geometry)
    assert mask.is_enabled(0, 0)
    assert not mask.is_enabled(1, 1)
    assert mask.num_enabled(geometry) == sum(sum(row) for row in rows)
    with pytest.raises(IndexError):
        mask.is_enabled(2, 0)
    with pytest.raises(IndexError):
        mask.is_enabled(0, Autd3.NUM_TRANSDUCERS)

    short = pattern.TransducerMask.masked(rows[:1])
    with pytest.raises(autd3.Autd3Error, match="device slots"):
        short.validate(geometry)


def test_transducer_groups_queries() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0]), Autd3([Autd3.DEVICE_WIDTH, 0.0, 0.0])])
    groups = pattern.TransducerGroups(geometry, lambda device, tr: "head" if tr < 10 else "tail")
    assert groups.keys() == ["head", "tail"]
    assert groups.num_devices() == 2
    assert groups.num_transducers(1) == Autd3.NUM_TRANSDUCERS
    assert groups.num_transducers_in("head") == 20
    assert groups.num_transducers_in("tail") == geometry.num_transducers() - 20
    assert groups.num_transducers_in("missing") == 0
    assert groups.indices(0) == [0] * 10 + [1] * (Autd3.NUM_TRANSDUCERS - 10)
    assert groups.index(1, 9) == 0
    assert groups.index(1, 10) == 1
    masks = groups.masks()
    assert [key for key, _ in masks] == groups.keys()
    assert [mask.num_enabled(geometry) for _, mask in masks] == [20, geometry.num_transducers() - 20]
    assert masks[0][1].is_enabled(1, 9)
    assert not masks[0][1].is_enabled(1, 10)
    with pytest.raises(IndexError):
        groups.indices(2)
    with pytest.raises(IndexError):
        groups.num_transducers(2)
    with pytest.raises(KeyError):
        groups.mask("missing")


def test_control_points_expose_their_fields() -> None:
    control_point = ControlPoint([1.0, 2.0, 3.0], Phase(0x20))
    np.testing.assert_array_equal(control_point.point, [1.0, 2.0, 3.0])
    assert control_point.phase_offset == Phase(0x20)
    assert ControlPoint([1.0, 2.0, 3.0]).phase_offset == Phase.ZERO
    assert control_point == ControlPoint([1.0, 2.0, 3.0], 0x20)
    assert control_point != ControlPoint([1.0, 2.0, 3.0])

    control_points = ControlPoints([control_point, ControlPoint([4.0, 5.0, 6.0])], Intensity(0x80))
    assert control_points.points == [control_point, ControlPoint([4.0, 5.0, 6.0])]
    assert control_points.intensity == Intensity(0x80)
    assert ControlPoints([control_point]).intensity == Intensity.MAX
    assert control_points == ControlPoints([control_point, ControlPoint([4.0, 5.0, 6.0])], 0x80)
    assert control_points != ControlPoints([control_point], 0x80)


def test_modulation_options_expose_their_fields() -> None:
    sine = modulation.SineOption()
    assert (sine.amplitude, sine.offset, sine.clamp) == (0xFF, 0x80, False)
    assert sine.phase == Angle.ZERO
    assert sine.sampling_config == SamplingConfig.FREQ_4K
    assert sine == modulation.SineOption()
    assert sine != modulation.SineOption(amplitude=0x80)
    assert "amplitude: 255" in repr(sine)
    custom = modulation.SineOption(phase=90 * deg, sampling_config=SamplingConfig.FREQ_40K)
    assert custom.phase == 90 * deg
    assert custom.sampling_config == SamplingConfig(1)

    square = modulation.SquareOption()
    assert (square.low, square.high, square.duty) == (0x00, 0xFF, 0.5)
    assert square.sampling_config == SamplingConfig.FREQ_4K
    assert square == modulation.SquareOption()
    assert square != modulation.SquareOption(duty=0.25)
    assert "duty: 0.5" in repr(square)

    fourier = modulation.FourierOption()
    assert (fourier.scale_factor, fourier.clamp, fourier.offset) == (None, False, 0x00)
    assert modulation.FourierOption(scale_factor=0.5).scale_factor == 0.5
    assert fourier == modulation.FourierOption()
    assert fourier != modulation.FourierOption(clamp=True)
    assert "clamp: false" in repr(fourier)

    component = modulation.SineComponent(100 * Hz, sine)
    assert component.freq == 100 * Hz
    assert isinstance(component.freq, Freq) and component.freq.is_int
    assert component.option == sine
    assert component == modulation.SineComponent(100 * Hz, modulation.SineOption())
    assert component != modulation.SineComponent(200 * Hz, sine)
    assert modulation.SineComponent(100.0 * Hz, sine).freq == 100.0 * Hz
    assert modulation.SineComponent(Nearest(150.0 * Hz), sine).freq == Nearest(150.0 * Hz)
    assert "SineComponent" in repr(component)


def test_modulation_buffer_numpy_round_trip() -> None:
    values = np.arange(8, dtype=np.uint8)
    buf = modulation.ModulationBuffer.from_array(values)
    assert len(buf) == 8
    assert repr(buf) == "ModulationBuffer(len=8)"
    out = buf.to_numpy()
    assert out.dtype == np.uint8
    np.testing.assert_array_equal(out, values)
    out[0] = 0xAA
    assert buf[0] == 0

    assert buf == modulation.ModulationBuffer.from_array(list(range(8)))
    assert buf == modulation.ModulationBuffer.from_bytes(bytes(range(8)))
    assert buf != modulation.ModulationBuffer(8)
    assert buf != list(range(8))

    buf.copy_from(values[::-2])
    np.testing.assert_array_equal(buf.to_numpy(), values[::-2])

    with pytest.raises(TypeError, match="uint8"):
        buf.copy_from(loose(values.astype(np.int64)))
    with pytest.raises(ValueError, match="shape"):
        buf.copy_from(loose(values.reshape(2, 4)))
    with pytest.raises(TypeError, match="ndarray"):
        buf.copy_from(loose([1, 2, 3]))
    with pytest.raises(TypeError):
        modulation.ModulationBuffer.from_array(loose("abc"))
    with pytest.raises(TypeError, match="uint8"):
        modulation.ModulationBuffer.from_array(loose(values.astype(np.float32)))
    np.testing.assert_array_equal(buf.to_numpy(), values[::-2])

    autd3.commands.Modulation(SamplingConfig.FREQ_4K, buf)


def test_modulation_names_follow_rust() -> None:
    buf = modulation.modulation_buffer()
    modulation.constant(amplitude=0x40, dst=buf)
    assert list(buf) == [0x40, 0x40]
    with pytest.raises(TypeError):
        loose(modulation.constant)(intensity=0x40, dst=buf)

    assert modulation.samples_per_period(10, 200 * Hz) == 20
    assert modulation.samples_per_period(10, 3 * Hz) is None
    assert modulation.samples_per_period(0, 200 * Hz) is None
    with pytest.raises(ValueError):
        modulation.samples_per_period(10, loose(200))
    with pytest.raises(ValueError):
        modulation.samples_per_period(10, 200.0 * Hz)


def test_amplitude_accessors_follow_rust() -> None:
    assert (2.5 * kPa).pascal == pytest.approx(2500.0)
    assert (121.5 * dB).spl == pytest.approx(121.5, abs=1e-3)
    assert not hasattr(1.0 * Pa, "as_pascal")
    assert not hasattr(1.0 * Pa, "as_spl")


def test_wavelength_is_a_length() -> None:
    wavelength = pattern.wavelength(340 * m / s)
    assert isinstance(wavelength, Length)
    assert wavelength == 8.5 * mm


def test_holo_batches_solve_one_problem_per_buffer() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0])])
    wavelength = pattern.wavelength(340 * m / s)
    center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm)
    problems = [
        [holo.AmplitudeTarget(center + offset(x * mm, 0.0 * mm, 0.0 * mm), 2.5e3 * Pa) for x in pair]
        for pair in ((-30.0, 30.0), (-10.0, 10.0))
    ]
    foci = [target for problem in problems for target in problem]

    for solve, solve_batch, option in (
        (holo.naive, holo.naive_batch, holo.NaiveOption()),
        (holo.gs, holo.gs_batch, holo.GsOption(repeat=10)),
        (holo.gspat, holo.gspat_batch, holo.GspatOption(repeat=10)),
    ):
        phases = [geometry.phase_buffer() for _ in problems]
        intensities = [geometry.intensity_buffer() for _ in problems]
        loose(solve_batch)(geometry, foci, wavelength, option, phases, intensities)
        for problem, batched_phases, batched_intensities in zip(problems, phases, intensities):
            single_phases = geometry.phase_buffer()
            single_intensities = geometry.intensity_buffer()
            loose(solve)(geometry, problem, wavelength, option, single_phases, single_intensities)
            diff = (batched_phases.to_numpy().astype(np.int16) - single_phases.to_numpy().astype(np.int16)) & 0xFF
            assert np.all((diff <= 1) | (diff == 0xFF))
            assert batched_intensities.num_devices() == 1
        assert not np.array_equal(phases[0].to_numpy(), phases[1].to_numpy())

    phases = [geometry.phase_buffer() for _ in problems]
    intensities = [geometry.intensity_buffer() for _ in problems]
    with pytest.raises(ValueError, match="twice"):
        holo.gs_batch(geometry, foci, wavelength, holo.GsOption(), [phases[0], phases[0]], intensities)
    with pytest.raises(holo.HoloError):
        holo.gs_batch(geometry, foci[:3], wavelength, holo.GsOption(), phases, intensities)
    with pytest.raises(holo.HoloError):
        holo.naive_batch(geometry, foci, wavelength, holo.NaiveOption(), [], [])
    assert all(buffer.num_devices() == 1 for buffer in phases)
    with pytest.raises(ValueError, match="Length"):
        holo.naive(geometry, foci, loose(8.5), holo.NaiveOption(), phases[0], intensities[0])


def test_frames_are_re_encoded_in_place() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0])])
    frames = autd3.Frames()
    assert frames.is_empty()
    assert len(frames) == 0

    frames.encode_into(geometry, autd3.commands.Nop())
    assert len(frames) == 1
    held = frames[0]
    frames.encode_into(geometry, (autd3.commands.Nop(), autd3.commands.Clear()))
    assert len(frames) == 2
    assert isinstance(held, autd3.Frame)

    invalid = autd3.commands.SetCpuConfig(CpuConfig(failsafe_timeout=autd3.Duration.from_nanos(0)))
    with pytest.raises(autd3.Autd3Error):
        frames.encode_into(geometry, invalid)
    assert frames.is_empty()
    with pytest.raises(TypeError):
        frames.encode_into(geometry, loose(1))


def test_signatures_are_introspectable() -> None:
    assert list(inspect.signature(pattern.focus).parameters) == ["geometry", "target", "wavelength", "dst"]
    assert list(inspect.signature(autd3.commands.FociStm).parameters) == ["config", "points", "option"]
    assert list(inspect.signature(modulation.constant).parameters) == ["amplitude", "dst"]
    assert list(inspect.signature(modulation.samples_per_period).parameters) == ["divider", "freq"]
    assert list(inspect.signature(autd3.commands.circle).parameters) == [
        "center",
        "radius",
        "num_points",
        "normal",
        "intensity",
        "dst",
    ]
