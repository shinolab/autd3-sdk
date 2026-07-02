"""Hardware-free tests: pattern/modulation/holo computation and datagram building."""

import numpy as np
import pytest

import autd3
import autd3_modulation as modulation
import autd3_pattern as pattern
import autd3_pattern_holo as holo
from autd3.units import Hz, kHz, deg, m, mm, rad, s
from autd3_pattern_holo import Pa, dB, kPa


def geometry() -> autd3.geometry.Geometry:
    return autd3.geometry.Geometry([autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])


def test_unit_dsl() -> None:
    assert 2 * kHz == 2000 * Hz
    assert 200 * Hz == 200.0 * Hz
    assert (200 * Hz).hz == 200.0
    assert (200 * Hz).is_int
    assert not (200.0 * Hz).is_int
    assert (340 * m / s) == (340_000 * mm / s)
    assert (340 * m / s).m_s == 340.0
    assert (340 * m / s).mm_per_s == 340_000.0
    assert (180 * deg).radian == pytest.approx(np.pi)
    assert (np.pi * rad).degree == pytest.approx(180.0)

    assert (2.5 * kPa).as_pascal == pytest.approx(2500.0)
    assert (2500.0 * Pa) == (2.5 * kPa)
    assert (121.5 * dB).as_pascal == pytest.approx(23.77, abs=1e-2)
    assert (23.77 * Pa).as_spl == pytest.approx(121.5, abs=1e-2)

    with pytest.raises(ValueError):
        pattern.wavelength(340_000.0)
    with pytest.raises(ValueError):
        modulation.sine(200.0, modulation.SineOption(), modulation.modulation_buffer())


def test_pattern_focus_plane_bessel_uniform_null() -> None:
    geo = geometry()
    wavelength = pattern.wavelength(340 * m / s)
    center = geo.center()
    buf = geo.pattern_buffer()

    pattern.focus(geo, center + np.array([0.0, 0.0, 150.0]), wavelength, pattern.FocusOption(), buf)
    pattern.plane(geo, [0.0, 0.0, 1.0], wavelength, pattern.PlaneOption(), buf)
    pattern.bessel(geo, center, [0.0, 0.0, 1.0], 0.3 * rad, wavelength, pattern.BesselOption(), buf)
    pattern.uniform(autd3.value.Emission(autd3.value.Phase(0x00), autd3.value.Intensity(0x80)), buf)
    pattern.null(buf)
    assert len(buf) == geo.num_devices()


def test_modulation_sine_square_fourier_radiation() -> None:
    buf = modulation.modulation_buffer()
    modulation.sine(200 * Hz, modulation.SineOption(), buf)
    assert len(buf) > 0

    sq = modulation.modulation_buffer()
    modulation.square(150 * Hz, modulation.SquareOption(), sq)
    assert len(sq) > 0

    c = modulation.modulation_buffer()
    modulation.constant(0xFF, c)
    assert len(c) == 2

    fo = modulation.modulation_buffer()
    modulation.fourier(
        [modulation.SineComponent(100 * Hz, modulation.SineOption()),
         modulation.SineComponent(200 * Hz, modulation.SineOption())],
        modulation.FourierOption(),
        fo,
    )
    assert len(fo) > 0

    rp = modulation.modulation_buffer()
    modulation.sine(200 * Hz, modulation.SineOption(), rp)
    before = len(rp)

    out = modulation.modulation_buffer()
    modulation.radiation_pressure(rp, out)
    assert len(out) == before
    assert len(rp) == before

    modulation.radiation_pressure_inplace(rp)
    assert len(rp) == before


def test_holo_algorithms() -> None:
    geo = geometry()
    wavelength = pattern.wavelength(340 * m / s)
    center = geo.center()
    foci = [
        holo.ControlPoint(center + np.array([-20.0, 0.0, 150.0]), 5e3 * Pa),
        holo.ControlPoint(center + np.array([20.0, 0.0, 150.0]), 150 * dB),
    ]
    buf = geo.pattern_buffer()
    holo.naive(geo, foci, wavelength, holo.NaiveOption(), buf)
    holo.gs(geo, foci, wavelength, holo.GsOption(repeat=10), buf)
    holo.gspat(geo, foci, wavelength, holo.GspatOption(repeat=10), buf)
    holo.greedy(geo, foci, wavelength, holo.GreedyOption(), buf)
    assert len(buf) == geo.num_devices()


def test_stm_foci_and_pattern() -> None:
    geo = geometry()
    builder = autd3.DatagramBuilder(geo.num_devices())

    samples = []
    autd3.commands.circle([0.0, 0.0, 150.0], 30.0, 8, [0.0, 0.0, 1.0], autd3.value.Intensity.MAX, samples)
    builder.push(autd3.commands.FociStm(1.0 * Hz, samples, autd3.commands.FociStmOption()))

    wavelength = pattern.wavelength(340 * m / s)
    frames = []
    for x in (-20.0, 20.0):
        buf = geo.pattern_buffer()
        pattern.focus(geo, geo.center() + np.array([x, 0.0, 150.0]), wavelength, pattern.FocusOption(), buf)
        frames.append(buf)
    builder.push(autd3.commands.PatternStm(autd3.commands.StmConfig(1.0 * Hz), frames, autd3.commands.PatternStmOption()))

    datagrams = builder.build()
    assert len(datagrams) > 0


def test_push_each() -> None:
    geo = autd3.geometry.Geometry([
        autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0]),
        autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0]),
    ])
    wavelength = pattern.wavelength(340 * m / s)
    left = geo.pattern_buffer()
    pattern.focus(geo, geo.center() + np.array([-40.0, 0.0, 150.0]), wavelength, pattern.FocusOption(), left)
    right = geo.pattern_buffer()
    pattern.focus(geo, geo.center() + np.array([40.0, 0.0, 150.0]), wavelength, pattern.FocusOption(), right)
    mod_buf = modulation.modulation_buffer()
    modulation.sine(150 * Hz, modulation.SineOption(), mod_buf)

    # homogeneous per-device command
    builder = autd3.DatagramBuilder(geo.num_devices())
    builder.push_each(lambda device: autd3.commands.Pattern(left if device % 2 == 0 else right))
    assert len(builder.build()) > 0

    # heterogeneous per-device command (Python is dynamically typed, no boxing needed)
    builder = autd3.DatagramBuilder(geo.num_devices())
    builder.push_each(
        lambda device: autd3.commands.Pattern(left)
        if device % 2 == 0
        else autd3.commands.Modulation(autd3.value.SamplingConfig.FREQ_4K, mod_buf)
    )
    assert len(builder.build()) > 0

    # returning None leaves that device unassigned
    builder = autd3.DatagramBuilder(geo.num_devices())
    builder.push_each(lambda device: autd3.commands.Pattern(left) if device == 0 else None)
    assert len(builder.build()) > 0


def test_commands_build() -> None:
    geo = geometry()
    n = geo.num_devices()
    builder = autd3.DatagramBuilder(n)
    builder.push(autd3.commands.Clear())
    builder.push(autd3.commands.Synchronize())
    builder.push(autd3.commands.ForceFan(True))
    builder.push(autd3.commands.SetSilencer(autd3.commands.FixedCompletionTime()))
    builder.push(autd3.commands.SetSilencer(autd3.commands.FixedUpdateRate(intensity=256, phase=256)))
    builder.push(autd3.commands.SetSilencer.disable())
    builder.push(autd3.commands.SetGpioOut([autd3.commands.GpioOut.Off, autd3.commands.GpioOut.BaseSignal,
                                   autd3.commands.GpioOut.PwmOut(0), autd3.commands.GpioOut.Direct(True)]))
    builder.push(autd3.commands.EmulateGpioIn([True, False, True, False]))
    assert len(builder.build()) > 0


def test_pulse_width_table_and_pulse_width() -> None:
    geo = geometry()
    builder = autd3.DatagramBuilder(geo.num_devices())

    table = autd3.value.PulseWidth.default_table()
    assert len(table) == 256
    assert autd3.commands.SetPulseWidthTable.default_table() == table
    builder.push(autd3.commands.SetPulseWidthTable(table))
    assert len(builder.build()) > 0

    assert autd3.value.PulseWidth.from_duty(0.5) == 256
    assert autd3.value.PulseWidth.from_duty(0.0) == 0
    with pytest.raises(ValueError):
        autd3.value.PulseWidth.from_duty(1.0)
    with pytest.raises(ValueError):
        autd3.value.PulseWidth.from_duty(-0.5)


def test_device_accessors() -> None:
    geo = geometry()
    assert geo.num_transducers() == geo[0].num_transducers()
    dev = geo[0]
    assert dev.idx() == 0
    assert len(dev.rotation()) == 4
    assert len(dev.x_direction()) == 3
    assert len(dev.y_direction()) == 3
    assert len(dev.axial_direction()) == 3
    assert len(dev.center()) == 3


def test_link_options_construct() -> None:
    import autd3_link_nop as nop
    import autd3_link_remote as remote
    import autd3_link_twincat as twincat

    remote.RemoteLinkOption("127.0.0.1:8080")
    remote.RemoteLinkOption("127.0.0.1:8080", timeout=autd3.Duration.from_millis(500))
    twincat.TwinCATLinkOption.local()
    twincat.TwinCATLinkOption.remote("169.254.1.1", "1.2.3.4.1.1")
    twincat.TwinCATLinkOption.local_with_timeouts(connect=autd3.Duration.from_millis(100))
    nop.Nop()


def test_loop_behavior_and_transition_mode() -> None:
    geo = geometry()
    wavelength = pattern.wavelength(340 * m / s)
    buf = geo.pattern_buffer()
    pattern.focus(geo, geo.center() + np.array([0.0, 0.0, 150.0]), wavelength, pattern.FocusOption(), buf)

    builder = autd3.DatagramBuilder(geo.num_devices())
    builder.push(autd3.commands.WritePatternBuffer(autd3.value.PatternBank.B1, 0, buf))
    builder.push(
        autd3.commands.ConfigPattern(
            autd3.value.PatternBank.B1,
            autd3.value.SamplingConfig.FREQ_4K,
            1,
            loop_behavior=autd3.value.LoopBehavior.Finite(5),
        )
    )
    builder.push(
        autd3.commands.ChangePatternBank(
            autd3.value.PatternBank.B1,
            transition_mode=autd3.value.TransitionMode.Gpio(autd3.value.GpioIn.I1),
        )
    )
    assert len(builder.build()) > 0
