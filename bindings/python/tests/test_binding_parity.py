import asyncio
from typing import Any

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
    Modulation,
    PatternStm,
    PatternStmOption,
    PhaseDepth,
    SetOutputMask,
    SetPhaseCorrection,
    SetSilencer,
    StmConfig,
    WriteFociBuffer,
)
from autd3.geometry import Autd3, Geometry, offset
from autd3.units import Angle, Freq, Hz, Length, deg, kHz, m, mm, rad, s
from autd3.value import (
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
    assert Phase(0x81) * 2 == Phase(0x02)
    assert 2 * Phase(0x81) == Phase(0x02)
    assert Phase(0x81) / 2 == Phase(0x40)
    assert Phase(0x81) // 2 == Phase(0x40)
    with pytest.raises(ZeroDivisionError):
        Phase(1) / 0
    with pytest.raises(TypeError):
        loose(Phase(1)) + 1


def test_intensity_arithmetic_saturates_like_rust() -> None:
    assert Intensity(0xF0) + Intensity(0x20) == Intensity.MAX
    assert Intensity(0x10) - Intensity(0x20) == Intensity.MIN
    assert Intensity(0x80) * 3 == Intensity.MAX
    assert 2 * Intensity(0x10) == Intensity(0x20)
    assert Intensity(0x81) / 2 == Intensity(0x40)
    assert Intensity(0x81) // 2 == Intensity(0x40)
    with pytest.raises(ZeroDivisionError):
        Intensity(1) // 0


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


def test_freq_arithmetic_keeps_integer_frequencies_integral() -> None:
    assert (100 * Hz + 50 * Hz).hz_int == 150
    assert (100 * Hz - 50 * Hz).hz_int == 50
    assert (100 * Hz * 2).hz_int == 200
    assert (2 * (100 * Hz)).hz_int == 200
    assert (201 * Hz // 2).hz_int == 100
    assert not (100 * Hz / 2).is_int
    assert (100 * Hz / 2).hz == pytest.approx(50.0)
    assert not (100 * Hz + 0.5 * Hz).is_int
    assert (100 * Hz * 1.5).hz == pytest.approx(150.0)
    assert 1 * kHz > 999 * Hz
    assert 100 * Hz <= 100.0 * Hz
    with pytest.raises(OverflowError):
        50 * Hz - 100 * Hz
    with pytest.raises(TypeError):
        loose(1.5 * Hz) // 2


def test_amplitude_arithmetic_and_ordering() -> None:
    assert (2 * (5 * kPa)).pascal == pytest.approx(10e3)
    assert ((5 * kPa) * 2).pascal == pytest.approx(10e3)
    assert ((5 * kPa) / 2).pascal == pytest.approx(2.5e3)
    assert 1 * Pa < 1 * kPa
    with pytest.raises(ZeroDivisionError):
        (1 * Pa) / 0


def test_constants_come_from_rust() -> None:
    assert params.ULTRASOUND_FREQ_HZ == 40000
    assert params.SIMULATOR_PORT == 44336
    assert params.PORT > 0
    assert params.DEVICE_QUEUE_FRAMES > 0
    assert params.ROTATION_NORM_TOLERANCE == Autd3.ROTATION_NORM_TOLERANCE


def test_autd3_defaults_to_the_origin_and_identity() -> None:
    assert Autd3() == Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])
    assert Autd3(rotation=[1.0, 0.0, 0.0, 0.0]).origin.tolist() == [0.0, 0.0, 0.0]
    with pytest.raises(TypeError):
        Autd3(loose(None))


def test_a_negative_device_index_is_an_index_error() -> None:
    geometry = one_device()
    with pytest.raises(IndexError):
        geometry[-1]
    with pytest.raises(IndexError):
        geometry[0][-1]


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
        lambda: autd3.TransportOption(reply_timeout=loose(None)),
        lambda: autd3.ClientConfig(max_inflight=loose(None)),
        lambda: FociStmOption(bank=loose(None)),
        lambda: FociStmOption(sound_speed=loose(None)),
        lambda: PatternStmOption(transition_mode=loose(None)),
        lambda: FixedCompletionTime(intensity=loose(None)),
        lambda: SetSilencer(loose(None)),
        lambda: autd3.commands.SetPulseWidthTable(loose(None)),
        lambda: holo.GspatOption(mask=loose(None)),
        lambda: holo.GreedyOption(mask=loose(None)),
        lambda: emu.RmsRecordOption(sound_speed=loose(None)),
        lambda: emu.InstantRecordOption(time_step=loose(None)),
    ],
)
def test_none_is_not_a_default_for_rust_non_option_fields(build: Any) -> None:
    with pytest.raises(TypeError):
        build()


def test_option_defaults_and_equality() -> None:
    assert autd3.TransportOption() == autd3.TransportOption()
    assert autd3.TransportOption() != autd3.TransportOption(reply_timeout=Duration.from_millis(2))
    assert FociStmOption() == FociStmOption(sound_speed=340 * m / s)
    assert FociStmOption() != FociStmOption(bank=PatternBank.B1)
    assert PatternStmOption() == PatternStmOption(phase_depth=PhaseDepth.Bits8)
    assert StmConfig(1.0 * Hz) == StmConfig(1.0 * Hz)
    assert StmConfig(1.0 * Hz) != StmConfig(2.0 * Hz)

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
    assert gspat.parallel is True
    assert gspat.mask.num_enabled(one_device()) == Autd3.NUM_TRANSDUCERS
    assert gspat == holo.GspatOption(mask=pattern.TransducerMask.AllEnabled)
    assert gspat != holo.GspatOption(repeat=10)
    assert "GspatOption" in repr(gspat)
    assert holo.GsOption().repeat == 100
    assert holo.NaiveOption() == holo.NaiveOption()
    masked = pattern.TransducerMask.masked([[False] * Autd3.NUM_TRANSDUCERS])
    assert holo.NaiveOption(mask=masked).mask.num_enabled(one_device()) == 0
    greedy = holo.GreedyOption()
    assert greedy.phase_quantization_levels == 16
    assert greedy.directivity == holo.Directivity.Sphere

    target = holo.AmplitudeTarget([1.0, 2.0, 3.0], 5 * kPa)
    assert target.point.tolist() == [1.0, 2.0, 3.0]
    assert target.amplitude == 5 * kPa
    assert target == holo.AmplitudeTarget([1.0, 2.0, 3.0], 5 * kPa)
    assert target != holo.AmplitudeTarget([1.0, 2.0, 3.0], 4 * kPa)


def test_holo_errors_carry_an_error_code() -> None:
    assert issubclass(holo.HoloError, autd3.Autd3Error)
    geometry = one_device()
    with pytest.raises(holo.HoloError) as err:
        holo.naive(
            geometry, [], pattern.wavelength(340 * m / s), holo.NaiveOption(), geometry.phase_buffer(), geometry.intensity_buffer()
        )
    assert err.value.code == autd3.Autd3Error.INVALID_ARGUMENT


def test_core_value_errors_are_invalid_arguments() -> None:
    with pytest.raises(autd3.Autd3Error) as err:
        SamplingConfig(3000.5 * Hz).freq()
    assert err.value.code == autd3.Autd3Error.INVALID_ARGUMENT
    mask = pattern.TransducerMask.masked([[True] * 3])
    with pytest.raises(autd3.Autd3Error) as err:
        mask.validate(one_device())
    assert err.value.code == autd3.Autd3Error.INVALID_ARGUMENT


def test_a_shared_intensity_buffer_is_shared_by_every_index() -> None:
    geometry = one_device()
    wavelength = pattern.wavelength(340 * m / s)
    phases = []
    for x in (-10, 10):
        buffer = geometry.phase_buffer()
        pattern.focus(geometry, geometry.center() + offset(x * mm, 0 * mm, 150 * mm), wavelength, buffer)
        phases.append(buffer)
    shared = geometry.intensity_buffer()
    pattern.set_intensity(Intensity(0x80), shared)
    assert len(autd3.Frames.encode(geometry, PatternStm(1.0 * Hz, phases, shared, PatternStmOption()))) > 0
    with pytest.raises(autd3.Autd3Error, match="intensit"):
        autd3.Frames.encode(geometry, PatternStm(1.0 * Hz, phases, [shared], PatternStmOption()))


def test_sampling_configs_are_resolved_when_encoded() -> None:
    geometry = one_device()
    unresolvable = SamplingConfig(3000.5 * Hz)
    commands: list[autd3.commands.Command] = [
        ConfigModulation(ModulationBank.B0, unresolvable, 2, LoopBehavior.Infinite),
        ConfigPattern(PatternBank.B0, unresolvable, 2, LoopBehavior.Infinite),
        ConfigFociStm(PatternBank.B0, unresolvable, 2, 1, 340 * m / s, LoopBehavior.Infinite),
    ]
    for command in commands:
        with pytest.raises(autd3.Autd3Error, match="ultrasound frequency") as err:
            autd3.Frames.encode(geometry, command)
        assert err.value.code == autd3.Autd3Error.INVALID_ARGUMENT

    config = StmConfig(unresolvable)
    with pytest.raises(autd3.Autd3Error, match="ultrasound frequency"):
        config.into_sampling_config(2).divide()

    buffer = modulation.modulation_buffer()
    with pytest.raises(autd3.Autd3Error):
        modulation.sine(150 * Hz, modulation.SineOption(), unresolvable, buffer)


def test_command_contents_are_checked_when_encoded() -> None:
    geometry = one_device()
    for command in (
        SetOutputMask([[True] * 3]),
        SetPhaseCorrection([[Phase.ZERO] * 3]),
        FociStm(1.0 * Hz, [], FociStmOption()),
        WriteFociBuffer(PatternBank.B0, 0, []),
    ):
        with pytest.raises(autd3.Autd3Error) as err:
            autd3.Frames.encode(geometry, loose(command))
        assert err.value.code == autd3.Autd3Error.INVALID_ARGUMENT
    with pytest.raises(ValueError):
        FociStm(1.0 * Hz, [ControlPoints([], 0xFF)], FociStmOption())


def test_a_zero_ack_timeout_is_rejected_at_open() -> None:
    config = autd3.ClientConfig(ack_timeout=Duration.from_nanos(0))

    async def open_client() -> None:
        await autd3.Client.open(one_device(), autd3.TransportOption(), config)

    with pytest.raises(autd3.Autd3Error, match="ack_timeout") as err:
        asyncio.run(open_client())
    assert err.value.code == autd3.Autd3Error.INVALID_ARGUMENT


def test_a_zero_lost_timeout_is_rejected_at_open() -> None:
    option = autd3.TransportOption(lost_timeout=Duration.from_nanos(0))

    async def open_client() -> None:
        await autd3.Client.open(one_device(), option, autd3.ClientConfig())

    with pytest.raises(autd3.Autd3Error, match="lost_timeout") as err:
        asyncio.run(open_client())
    assert err.value.code == autd3.Autd3Error.NETWORK


def test_pattern_functions_reject_a_buffer_for_another_geometry() -> None:
    geometry = one_device()
    two = Geometry([Autd3(), Autd3([Autd3.DEVICE_WIDTH, 0.0, 0.0])])
    wavelength = pattern.wavelength(340 * m / s)
    with pytest.raises(ValueError, match="one slot per device"):
        pattern.focus(geometry, [0.0, 0.0, 150.0], wavelength, two.phase_buffer())
    with pytest.raises(ValueError, match="one slot per device"):
        pattern.plane(geometry, [0.0, 0.0, 1.0], wavelength, two.phase_buffer())


def test_radiation_pressure_needs_distinct_buffers() -> None:
    buffer = modulation.modulation_buffer()
    modulation.constant(0x80, buffer)
    with pytest.raises(ValueError, match="radiation_pressure_inplace"):
        modulation.radiation_pressure(buffer, buffer)
    dst = modulation.modulation_buffer()
    modulation.radiation_pressure(buffer, dst)
    assert len(dst) == len(buffer)


def test_emulator_options_default_to_rust() -> None:
    instant = emu.InstantRecordOption()
    assert instant.time_step == Duration.from_micros(1)
    assert instant.memory_limits_hint_mb == 128
    instant.time_step = Duration.from_micros(5)
    assert instant.time_step == Duration.from_micros(5)
    assert emu.RmsRecordOption().sound_speed == 340 * m / s


def test_the_emulator_reports_its_device_count_by_method() -> None:
    emulator = autd3.UdpEmulator(1)
    assert emulator.num_devices() == 1


def test_unit_types_are_reexported() -> None:
    assert isinstance(1 * Hz, Freq)
    assert isinstance(1 * mm, Length)
    assert isinstance(1 * deg, Angle)
