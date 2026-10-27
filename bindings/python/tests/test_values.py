import inspect
from collections.abc import Hashable
from typing import Any

import numpy as np
import pytest

import autd3
import autd3_modulation as modulation
import autd3_pattern as pattern
import autd3_pattern_holo as holo
from autd3.commands import CpuConfig, FpgaBusWait, FpgaConfig, GpioOut, PhaseDepth, PtpConfig
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
    (LoopBehavior.Once, LoopBehavior.Finite(1)),
    (LoopBehavior.Infinite, LoopBehavior.Infinite),
    (GpioIn.I2, GpioIn.I2),
    (PatternBank.B1, PatternBank.B1),
    (ModulationBank.B0, ModulationBank.B0),
    (Nearest(150.0 * Hz), Nearest(150 * Hz)),
    (autd3.Duration.from_millis(1), autd3.Duration.from_micros(1000)),
    (GpioOut.PwmOut(3), GpioOut.PwmOut(3)),
    (GpioOut.Sync, GpioOut.Sync),
    (autd3.ExecAt.Immediate, autd3.ExecAt.Immediate),
    (autd3.ExecAt.Lead(autd3.Duration.from_millis(1)), autd3.ExecAt.Lead(autd3.Duration.from_micros(1000))),
    (autd3.ExecAt.At(SysTime.from_nanos(5)), autd3.ExecAt.At(SysTime.from_nanos(5))),
    (FpgaBusWait.Cycles2, FpgaBusWait.Cycles2),
    (PtpConfig(), PtpConfig(kp_milli=PtpConfig().kp_milli)),
    (CpuConfig(), CpuConfig(ptp=PtpConfig())),
    (FpgaConfig(), FpgaConfig(sync_phase_lock=True)),
    (PhaseDepth.Bits4, PhaseDepth.Bits4),
    (holo.IntensityConstraint.Multiply(0.5), holo.IntensityConstraint.Multiply(0.5)),
    (holo.IntensityConstraint.Clamp(1, 2), holo.IntensityConstraint.Clamp(1, 2)),
    (holo.Directivity.T4010A1, holo.Directivity.T4010A1),
    (2500.0 * Pa, 2.5 * kPa),
    (autd3.Interface.Name("eth0"), autd3.Interface.Name("eth0")),
    (10 * mm, Length.from_m(0.01)),
    (0.0 * mm, -0.0 * mm),
    (2 * kHz, 2000 * Hz),
    (2.0 * kHz, 2000.0 * Hz),
    (200 * Hz, 200.0 * Hz),
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
    (TransitionMode.Immediate, TransitionMode.Ext),
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
    (autd3.ExecAt.Lead(autd3.Duration.from_millis(1)), autd3.ExecAt.Lead(autd3.Duration.from_millis(2))),
    (autd3.ExecAt.At(SysTime.from_nanos(5)), autd3.ExecAt.At(SysTime.from_nanos(6))),
    (autd3.ExecAt.Immediate, autd3.ExecAt.At(SysTime.ZERO)),
    (autd3.ExecAt.Lead(autd3.Duration.from_nanos(5)), autd3.ExecAt.At(SysTime.from_nanos(5))),
    (FpgaBusWait.Cycles2, FpgaBusWait.Cycles3),
    (PtpConfig(), PtpConfig(kp_milli=PtpConfig().kp_milli + 1)),
    (CpuConfig(), CpuConfig(failsafe_timeout=autd3.Duration.from_millis(500))),
    (FpgaConfig(), FpgaConfig(sync_phase_lock=False)),
    (holo.IntensityConstraint.Multiply(0.5), holo.IntensityConstraint.Multiply(0.25)),
    (holo.IntensityConstraint.Multiply(1.0), holo.IntensityConstraint.Clamp(0, 255)),
    (holo.Directivity.Sphere, holo.Directivity.T4010A1),
    (1.0 * Pa, 2.0 * Pa),
    (autd3.Interface.Auto, autd3.Interface.Name("eth0")),
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


@pytest.mark.parametrize(
    ("value", "message"),
    [
        (3000 * Hz, "must divide the ultrasound frequency"),
        (3000.5 * Hz, "must divide the ultrasound frequency"),
        (80_000 * Hz, "is out of range"),
        (0.0 * Hz, "is out of range"),
        (autd3.Duration.from_nanos(50_001), "must be a multiple of the ultrasound period"),
        (autd3.Duration.from_nanos(1), "is out of range"),
        (float("nan") * Hz, "is out of range"),
        (0, "non-zero"),
    ],
)
def test_an_invalid_sampling_config_cannot_be_constructed(value: object, message: str) -> None:
    with pytest.raises(autd3.Autd3Error, match=message) as err:
        SamplingConfig(loose(value))
    assert err.value.code == autd3.Autd3Error.INVALID_ARGUMENT


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
    assert config.freq() == 4000.0 * Hz
    assert isinstance(config.freq(), Freq)


def test_units_have_explicit_constructors() -> None:
    assert Length.from_mm(10.0) == 10 * mm
    assert Length.from_m(1.0) == 1000 * mm
    assert Freq.from_hz(200) == 200 * Hz
    assert Freq.from_hz(200.0) == 200 * Hz
    assert Angle.ZERO.rad == 0.0


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
    assert not hasattr(TransitionMode, "Later")


def test_params_mirror_the_rust_constants() -> None:
    params = autd3.params
    assert params.ULTRASOUND_PERIOD == SamplingConfig.FREQ_40K.period()
    assert Autd3.NUM_TRANSDUCERS == Geometry([Autd3([0.0, 0.0, 0.0])]).num_transducers()
    assert Autd3.GRID_X * Autd3.GRID_Y - 3 == Autd3.NUM_TRANSDUCERS
    assert params.MAX_DEVICES >= 1
    assert params.PWE_TABLE_SIZE == len(autd3.commands.SetPulseWidthTable.empty_table())
    assert params.BUFFER_SIZE_MIN >= 1
    assert params.NUM_FOCI_MAX >= 1
    assert params.EMISSION_MAX_INDICES >= params.BUFFER_SIZE_MIN
    assert params.MOD_BUFFER_SAMPLES >= params.BUFFER_SIZE_MIN
    assert params.PULSE_WIDTH_PERIOD > 0
    assert set(params.__all__) <= set(dir(params))
    for name in ("NUM_TRANSDUCERS", "GRID_X", "GRID_Y", "PITCH_MM", "ROTATION_NORM_TOLERANCE", "DEVICE_QUEUE_FRAMES"):
        assert not hasattr(params, name)
    for name in ("MAX_DEVICES", "MAX_INFLIGHT"):
        assert not hasattr(autd3, name)


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
    np.testing.assert_allclose(devices[1].position(1), [Autd3.DEVICE_WIDTH + Autd3.PITCH_MM, 0.0, 0.0])
    with pytest.raises(IndexError):
        devices[1].position(Autd3.NUM_TRANSDUCERS)
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


def test_control_points_expose_their_fields() -> None:
    control_point = ControlPoint([1.0, 2.0, 3.0], Phase(0x20))
    np.testing.assert_array_equal(control_point.point, [1.0, 2.0, 3.0])
    assert control_point.phase_offset == Phase(0x20)
    assert control_point == ControlPoint([1.0, 2.0, 3.0], 0x20)
    assert control_point != ControlPoint([1.0, 2.0, 3.0], Phase.ZERO)
    with pytest.raises(TypeError):
        loose(ControlPoint)([1.0, 2.0, 3.0])

    control_points = ControlPoints([control_point, ControlPoint([4.0, 5.0, 6.0], Phase.ZERO)], Intensity(0x80))
    assert control_points.points == [control_point, ControlPoint([4.0, 5.0, 6.0], Phase.ZERO)]
    assert control_points.intensity == Intensity(0x80)
    assert ControlPoints([control_point]) == ControlPoints([control_point], Intensity.MAX)
    assert ControlPoints(control_point) == ControlPoints([control_point], Intensity.MAX)
    assert control_points == ControlPoints([control_point, ControlPoint([4.0, 5.0, 6.0], Phase.ZERO)], 0x80)
    assert control_points != ControlPoints([control_point], 0x80)


def test_control_points_from_an_n_by_3_ndarray() -> None:
    rows = np.array([[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]])
    expected = ControlPoints([ControlPoint([1.0, 2.0, 3.0], Phase.ZERO), ControlPoint([4.0, 5.0, 6.0], Phase.ZERO)], 0x80)
    assert ControlPoints(rows, Intensity(0x80)) == expected
    assert ControlPoints(rows.astype(np.float32), 0x80) == expected
    assert ControlPoints(rows.astype(np.int64), 0x80) == expected
    assert ControlPoints(np.asfortranarray(rows), 0x80) == expected
    assert ControlPoints(np.zeros((0, 3)), 0x80).points == []
    assert ControlPoints(rows) == ControlPoints(rows, Intensity.MAX)
    assert ControlPoints(rows[0]) == ControlPoints([ControlPoint([1.0, 2.0, 3.0], Phase.ZERO)], Intensity.MAX)
    with pytest.raises(ValueError, match=r"shape \(3,\) or \(n, 3\)"):
        ControlPoints(np.zeros(2), 0x80)
    with pytest.raises(ValueError, match=r"shape \(3,\) or \(n, 3\)"):
        ControlPoints(np.zeros((2, 2)), 0x80)
    with pytest.raises(TypeError, match="real-valued dtype"):
        ControlPoints(np.zeros((2, 3), dtype=np.complex64), 0x80)
    with pytest.raises(TypeError, match="masked arrays"):
        ControlPoints(np.ma.masked_array(rows), 0x80)
    with pytest.raises(TypeError, match="a ControlPoint, a sequence of ControlPoint"):
        loose(ControlPoints)([[1.0, 2.0, 3.0]], 0x80)


def test_modulation_options_expose_their_fields() -> None:
    sine = modulation.SineOption()
    assert (sine.amplitude, sine.offset, sine.clamp) == (0xFF, 0x80, False)
    assert sine.phase == Angle.ZERO
    assert sine == modulation.SineOption()
    assert sine != modulation.SineOption(amplitude=0x80)
    assert "amplitude: 255" in repr(sine)
    custom = modulation.SineOption(phase=90 * deg)
    assert custom.phase == 90 * deg

    square = modulation.SquareOption()
    assert (square.low, square.high, square.duty) == (0x00, 0xFF, 0.5)
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
    assert isinstance(component.freq, Freq)
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
    assert buf == modulation.ModulationBuffer.from_array(bytes(range(8)))
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
    assert modulation.samples_per_period(10, 200.0 * Hz) == 20
    assert modulation.samples_per_period(10, 0.5 * Hz) == 8000
    with pytest.raises(ValueError):
        modulation.samples_per_period(10, loose(200))


def test_amplitude_accessors_follow_rust() -> None:
    assert (2.5 * kPa).pascal == pytest.approx(2500.0)
    assert (121.5 * dB).spl == pytest.approx(121.5, abs=1e-3)
    assert not hasattr(1.0 * Pa, "as_pascal")
    assert not hasattr(1.0 * Pa, "as_spl")


def test_wavelength_is_a_length() -> None:
    wavelength = pattern.wavelength(340 * m / s)
    assert isinstance(wavelength, Length)
    assert wavelength == 8.5 * mm


def test_holo_wavelength_must_be_a_length() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0])])
    foci = [holo.AmplitudeTarget(geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm), 2.5e3 * Pa)]
    with pytest.raises(TypeError, match="Length"):
        holo.naive(geometry, foci, loose(8.5), holo.NaiveOption(), geometry.phase_buffer(), geometry.intensity_buffer())


def test_frames_are_re_encoded_in_place() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0])])
    frames = autd3.Frames()
    assert frames.is_empty()
    assert len(frames) == 0

    frames.encode_into(geometry, autd3.commands.Nop())
    assert len(frames) == 1
    held = frames[0]
    frames.encode_into(geometry, autd3.commands.each(lambda _: autd3.commands.Clear()))
    assert len(frames) == 1
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
