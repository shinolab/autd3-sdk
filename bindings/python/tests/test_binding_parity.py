from typing import Any

import numpy as np
import pytest

import autd3
import autd3_emulator as emu
import autd3_modulation as modulation
import autd3_pattern as pattern
import autd3_pattern_holo as holo
from autd3 import params
from autd3.commands import (
    ActivateModulationBank,
    ActivatePatternBank,
    ConfigFociStm,
    ConfigModulation,
    ConfigPattern,
    FixedCompletionTime,
    FixedUpdateRate,
    FociStm,
    FociStmOption,
    GpioOut,
    Modulation,
    PatternStm,
    PatternStmOption,
    PhaseDepth,
    SetGpioOut,
    SetOutputMask,
    SetPhaseCorrection,
    SetSilencer,
    WriteFociBuffer,
)
from autd3.geometry import Autd3, Geometry, offset
from autd3.units import Angle, Freq, Hz, Length, deg, kHz, m, mm, rad, s
from autd3.value import (
    ControlPoint,
    ControlPoints,
    Intensity,
    LoopBehavior,
    ModulationBank,
    PatternBank,
    Phase,
    SamplingConfig,
    TransitionMode,
)
from autd3_core import Duration, Velocity
from autd3_pattern_holo import Pa, kPa


def loose(value: object) -> Any:
    return value


def one_device() -> Geometry:
    return Geometry([Autd3()])


def test_phase_arithmetic_wraps_like_rust() -> None:
    assert Phase(0xF0) + Phase(0x20) == Phase(0x10)
    assert Phase(0x10) - Phase(0x20) == Phase(0xF0)
    with pytest.raises(TypeError):
        loose(Phase(1)) + 1
    with pytest.raises(TypeError):
        loose(Phase(0x81)) * 2


def test_intensity_has_no_arithmetic_like_rust() -> None:
    assert Intensity(0x10) < Intensity(0x20)
    with pytest.raises(TypeError):
        loose(Intensity(0xF0)) + Intensity(0x20)
    with pytest.raises(TypeError):
        loose(Intensity(0x80)) * 3


def test_length_arithmetic_and_ordering() -> None:
    wavelength = 8.5 * mm
    assert 2 * wavelength == 17.0 * mm
    assert wavelength * 2 == 17.0 * mm
    assert wavelength / 2 == 4.25 * mm
    assert wavelength + 1.5 * mm == 10.0 * mm
    assert wavelength - 0.5 * mm == 8.0 * mm
    assert -wavelength == -8.5 * mm
    assert isinstance(340 * m / s, Velocity)
    assert 1 * mm < 1 * m
    assert 1 * m >= 1000 * mm
    with pytest.raises(ZeroDivisionError):
        wavelength / 0


def test_angle_arithmetic_without_ordering() -> None:
    assert (90 * deg + 90 * deg).deg == pytest.approx(180.0)
    assert (180 * deg - 90 * deg).deg == pytest.approx(90.0)
    assert (-(1 * rad)).rad == pytest.approx(-1.0)
    assert (2 * (1 * rad)).rad == pytest.approx(2.0)
    assert ((1 * rad) * 2).rad == pytest.approx(2.0)
    assert ((1 * rad) / 2).rad == pytest.approx(0.5)
    with pytest.raises(TypeError):
        loose(1 * rad) < loose(2 * rad)


def test_freq_is_a_single_float_like_rust() -> None:
    assert 100 * Hz + 50 * Hz == 150.0 * Hz
    assert 100 * Hz - 50 * Hz == 50.0 * Hz
    assert 100 * Hz * 2 == 200.0 * Hz
    assert 2 * (100 * Hz) == 200.0 * Hz
    assert (100 * Hz / 2).hz == pytest.approx(50.0)
    assert (100 * Hz * 1.5).hz == pytest.approx(150.0)
    assert (50 * Hz - 100 * Hz).hz == pytest.approx(-50.0)
    assert 1 * kHz > 999 * Hz
    assert 100 * Hz == 100.0 * Hz
    with pytest.raises(TypeError):
        loose(100 * Hz) // 2


def test_amplitude_arithmetic_and_ordering() -> None:
    assert (2 * (5 * kPa)).pascal == pytest.approx(10e3)
    assert ((5 * kPa) * 2).pascal == pytest.approx(10e3)
    assert ((5 * kPa) / 2).pascal == pytest.approx(2.5e3)
    assert 1 * Pa < 1 * kPa
    with pytest.raises(ZeroDivisionError):
        (1 * Pa) / 0


def test_constants_come_from_rust() -> None:
    assert params.ULTRASOUND_FREQ_HZ == 40000
    assert params.PORT > 0
    assert 0.0 < Autd3.ROTATION_NORM_TOLERANCE < 1.0


def test_autd3_defaults_to_the_origin_and_identity() -> None:
    assert Autd3() == Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])
    assert Autd3(rotation=[1.0, 0.0, 0.0, 0.0]).origin.tolist() == [0.0, 0.0, 0.0]
    with pytest.raises(TypeError):
        Autd3(loose(None))


def test_a_negative_device_index_is_an_index_error() -> None:
    geometry = one_device()
    with pytest.raises(IndexError):
        geometry[-1]


def test_rust_required_fields_cannot_be_omitted() -> None:
    config = SamplingConfig.FREQ_4K
    with pytest.raises(TypeError):
        loose(ConfigPattern)(PatternBank.B0, config, 2)
    with pytest.raises(TypeError):
        loose(ConfigModulation)(ModulationBank.B0, config, 2)
    with pytest.raises(TypeError):
        loose(ConfigFociStm)(PatternBank.B0, config, 2, 1, 340 * m / s)
    with pytest.raises(TypeError):
        loose(ActivatePatternBank)(PatternBank.B0)
    with pytest.raises(TypeError):
        loose(ActivateModulationBank)(ModulationBank.B0)
    with pytest.raises(TypeError):
        loose(PatternStm)(1.0 * Hz, [], 0xFF)
    ConfigPattern(PatternBank.B0, config, 2, LoopBehavior.Infinite)
    ConfigModulation(ModulationBank.B0, config, 2, LoopBehavior.Infinite)
    ConfigFociStm(PatternBank.B0, config, 2, 1, 340 * m / s, LoopBehavior.Infinite)
    ActivatePatternBank(PatternBank.B0, TransitionMode.Immediate)
    ActivateModulationBank(ModulationBank.B0, TransitionMode.Immediate)


@pytest.mark.parametrize(
    "build",
    [
        lambda: autd3.TransportOption(lost_timeout=loose(None)),
        lambda: autd3.TransportOption(ack_timeout=loose(None)),
        lambda: FociStmOption(bank=loose(None)),
        lambda: FociStmOption(sound_speed=loose(None)),
        lambda: PatternStmOption(transition_mode=loose(None)),
        lambda: FixedCompletionTime(intensity=loose(None)),
        lambda: SetSilencer(loose(None)),
        lambda: autd3.commands.SetPulseWidthTable(loose(None)),
        lambda: holo.GspatOption(mask=loose(None)),
        lambda: emu.RmsRecordOption(sound_speed=loose(None)),
        lambda: emu.InstantRecordOption(time_step=loose(None)),
    ],
)
def test_none_is_not_a_default_for_rust_non_option_fields(build: Any) -> None:
    with pytest.raises(TypeError):
        build()


def test_option_defaults_and_equality() -> None:
    assert autd3.TransportOption() == autd3.TransportOption()
    assert autd3.TransportOption() != autd3.TransportOption(lost_timeout=Duration.from_millis(2))
    assert FociStmOption() == FociStmOption(sound_speed=340 * m / s)
    assert FociStmOption() != FociStmOption(bank=PatternBank.B1)
    assert PatternStmOption() == PatternStmOption(phase_depth=PhaseDepth.Bits8)

    fixed = FixedCompletionTime()
    assert fixed.intensity == Duration.from_micros(250)
    assert fixed.phase == Duration.from_micros(1000)
    assert fixed.strict_mode is True
    assert fixed == FixedCompletionTime(strict_mode=True)
    assert fixed != FixedCompletionTime(strict_mode=False)
    rate = FixedUpdateRate(intensity=3, phase=4)
    assert (rate.intensity, rate.phase) == (3, 4)
    assert rate == FixedUpdateRate(3, 4)
    SetSilencer()


def test_pattern_options_are_frozen_values() -> None:
    lg = pattern.LaguerreGaussianOption(p=0, l=1, waist=10 * mm)
    assert lg == pattern.LaguerreGaussianOption(p=0, l=1, waist=10 * mm)
    assert lg != pattern.LaguerreGaussianOption(p=0, l=2, waist=10 * mm)
    assert "LaguerreGaussianOption" in repr(lg)
    hg = pattern.HermiteGaussianOption(m=1, n=0, waist=10 * mm)
    assert hg == pattern.HermiteGaussianOption(m=1, n=0, waist=10 * mm)
    assert "HermiteGaussianOption" in repr(hg)
    with pytest.raises(AttributeError):
        loose(lg).p = 1


def test_holo_options_expose_their_fields() -> None:
    gspat = holo.GspatOption()
    assert gspat.repeat == 100
    assert gspat.constraint == holo.IntensityConstraint.Clamp(0x00, 0xFF)
    assert gspat.directivity == holo.Directivity.Sphere
    assert gspat.mask.num_enabled(one_device()) == Autd3.NUM_TRANSDUCERS
    assert gspat == holo.GspatOption(mask=pattern.TransducerMask.AllEnabled)
    assert gspat != holo.GspatOption(repeat=10)
    assert "GspatOption" in repr(gspat)
    assert holo.GsOption().repeat == 100
    assert holo.NaiveOption() == holo.NaiveOption()
    masked = pattern.TransducerMask.Masked([[False] * Autd3.NUM_TRANSDUCERS])
    assert holo.NaiveOption(mask=masked).mask.num_enabled(one_device()) == 0
    greedy = holo.GreedyOption()
    assert greedy.phase_quantization_levels == 16
    assert greedy.directivity == holo.Directivity.Sphere

    target = holo.AmplitudeTarget([1.0, 2.0, 3.0], 5 * kPa)
    assert target.point.tolist() == [1.0, 2.0, 3.0]
    assert target.amplitude == 5 * kPa
    assert target == holo.AmplitudeTarget([1.0, 2.0, 3.0], 5 * kPa)
    assert target != holo.AmplitudeTarget([1.0, 2.0, 3.0], 4 * kPa)


def test_errors_form_one_hierarchy() -> None:
    assert issubclass(autd3.InvalidArgumentError, autd3.Autd3Error)
    assert issubclass(autd3.InvalidArgumentError, ValueError)
    assert issubclass(autd3.ReplyTimeoutError, autd3.Autd3Error)
    assert issubclass(autd3.ReplyTimeoutError, TimeoutError)
    assert issubclass(autd3.DeviceError, autd3.Autd3Error)
    assert issubclass(autd3.NetworkError, autd3.Autd3Error)
    assert not hasattr(holo, "HoloError")
    assert not hasattr(autd3.Autd3Error, "code")


def test_holo_errors_are_invalid_arguments() -> None:
    geometry = one_device()
    with pytest.raises(autd3.InvalidArgumentError):
        holo.naive(
            geometry, [], pattern.wavelength(340 * m / s), holo.NaiveOption(), geometry.phase_buffer(), geometry.intensity_buffer()
        )


def test_modulation_errors_are_invalid_arguments() -> None:
    with pytest.raises(autd3.InvalidArgumentError, match="duty"):
        modulation.square(150 * Hz, modulation.SquareOption(duty=2.0), SamplingConfig.FREQ_4K, modulation.ModulationBuffer(10))


def test_validations_the_rust_type_system_does_are_invalid_arguments() -> None:
    with pytest.raises(autd3.InvalidArgumentError, match="loop count"):
        LoopBehavior.Finite(0)
    with pytest.raises(autd3.InvalidArgumentError, match="exactly 4"):
        SetGpioOut([GpioOut.Off])


def test_type_and_state_errors_stay_builtin() -> None:
    with pytest.raises(TypeError, match="SamplingConfig expects"):
        SamplingConfig(loose("4000"))
    with pytest.raises(TypeError, match="expected a number"):
        Freq.from_hz(loose("x"))
    with pytest.raises(autd3.InvalidArgumentError, match="0-255"):
        Phase(300)
    with pytest.raises(autd3.InvalidArgumentError, match="at most 65535"):
        SamplingConfig(70_000)


def test_core_value_errors_are_invalid_arguments() -> None:
    with pytest.raises(autd3.InvalidArgumentError):
        SamplingConfig(3000.5 * Hz)
    mask = pattern.TransducerMask.Masked([[True] * 3])
    with pytest.raises(autd3.InvalidArgumentError):
        mask.validate(one_device())


def test_a_shared_intensity_buffer_is_shared_by_every_index() -> None:
    geometry = one_device()
    wavelength = pattern.wavelength(340 * m / s)
    phases = []
    for x in (-10, 10):
        buffer = geometry.phase_buffer()
        pattern.focus(geometry, geometry.center() + offset(x * mm, 0 * mm, 150 * mm), wavelength, buffer)
        phases.append(buffer)
    shared = geometry.intensity_buffer()
    shared.copy_from(np.full_like(shared.to_numpy(), 0x80))
    config = SamplingConfig(1.0 * Hz * len(phases))
    assert len(autd3.Frames.encode(geometry, PatternStm(config, phases, shared, PatternStmOption()))) > 0
    with pytest.raises(autd3.Autd3Error, match="intensit"):
        autd3.Frames.encode(geometry, PatternStm(config, phases, [shared], PatternStmOption()))


def test_sampling_configs_are_resolved_when_constructed() -> None:
    with pytest.raises(autd3.InvalidArgumentError, match="ultrasound frequency"):
        SamplingConfig(3000.5 * Hz)
    with pytest.raises(autd3.InvalidArgumentError, match="ultrasound period"):
        SamplingConfig(Duration.from_nanos(50_001))
    with pytest.raises(autd3.InvalidArgumentError, match="out of range"):
        SamplingConfig(80_000 * Hz)
    with pytest.raises(autd3.InvalidArgumentError, match="non-zero"):
        SamplingConfig(0)
    with pytest.raises(TypeError, match="SamplingConfig expects"):
        SamplingConfig(loose("4000"))

    geometry = one_device()
    resolved = SamplingConfig(13)
    commands: list[autd3.commands.Command] = [
        ConfigModulation(ModulationBank.B0, resolved, 2, LoopBehavior.Infinite),
        ConfigPattern(PatternBank.B0, resolved, 2, LoopBehavior.Infinite),
        ConfigFociStm(PatternBank.B0, resolved, 2, 1, 340 * m / s, LoopBehavior.Infinite),
    ]
    for command in commands:
        assert len(autd3.Frames.encode(geometry, command)) > 0


def test_command_contents_are_checked_when_encoded() -> None:
    geometry = one_device()
    point = ControlPoint([0.0, 0.0, 150.0], Phase.ZERO)
    for command in (
        SetOutputMask([[True] * 3]),
        FociStm(SamplingConfig.FREQ_4K, [], FociStmOption()),
        WriteFociBuffer(PatternBank.B0, 0, []),
        FociStm(SamplingConfig.FREQ_4K, [ControlPoints([], 0xFF)], FociStmOption()),
        FociStm(SamplingConfig.FREQ_4K, [ControlPoints([point], 0xFF), ControlPoints([point, point], 0xFF)], FociStmOption()),
        WriteFociBuffer(PatternBank.B0, 0, [ControlPoints([point] * 9, 0xFF)]),
    ):
        with pytest.raises(autd3.InvalidArgumentError):
            autd3.Frames.encode(geometry, loose(command))


def test_set_phase_correction_takes_a_phase_buffer_of_the_geometry() -> None:
    geometry = one_device()
    two = Geometry([Autd3(), Autd3([Autd3.DEVICE_WIDTH, 0.0, 0.0])])
    command = SetPhaseCorrection(geometry.phase_buffer())
    assert len(autd3.Frames.encode(geometry, command)) == 1
    with pytest.raises(autd3.InvalidArgumentError):
        autd3.Frames.encode(two, command)


def test_pattern_functions_reject_a_buffer_for_another_geometry() -> None:
    geometry = one_device()
    two = Geometry([Autd3(), Autd3([Autd3.DEVICE_WIDTH, 0.0, 0.0])])
    wavelength = pattern.wavelength(340 * m / s)
    with pytest.raises(autd3.InvalidArgumentError, match="one slot per device"):
        pattern.focus(geometry, [0.0, 0.0, 150.0], wavelength, two.phase_buffer())
    with pytest.raises(autd3.InvalidArgumentError, match="one slot per device"):
        pattern.plane(geometry, [0.0, 0.0, 1.0], wavelength, two.phase_buffer())


def test_emulator_options_default_to_rust() -> None:
    instant = emu.InstantRecordOption()
    assert instant.time_step == Duration.from_micros(1)
    assert instant.memory_limits_hint_mb == 128
    instant.time_step = Duration.from_micros(5)
    assert instant.time_step == Duration.from_micros(5)
    assert emu.RmsRecordOption().sound_speed == 340 * m / s


def test_unit_types_are_reexported() -> None:
    assert isinstance(1 * Hz, Freq)
    assert isinstance(1 * mm, Length)
    assert isinstance(1 * deg, Angle)
