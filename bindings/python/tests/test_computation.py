"""Hardware-free tests: pattern/modulation/holo computation and datagram building."""

import numpy as np
import pytest

import autd3
import autd3_modulation as modulation
import autd3_pattern as pattern
import autd3_pattern_holo as holo


def geometry() -> autd3.Geometry:
    return autd3.Geometry([autd3.Autd3()])


def test_pattern_focus_plane_bessel_uniform_null() -> None:
    geo = geometry()
    wavelength = pattern.wavelength(340_000.0)
    center = geo.center()
    buf = pattern.PatternBuffer(geo.num_devices())

    pattern.focus(geo, center + np.array([0.0, 0.0, 150.0]), wavelength, pattern.FocusOption(), buf)
    pattern.plane(geo, [0.0, 0.0, 1.0], wavelength, pattern.PlaneOption(), buf)
    pattern.bessel(geo, center, [0.0, 0.0, 1.0], 0.3, wavelength, pattern.BesselOption(), buf)
    pattern.uniform(0x80, 0x00, buf)
    pattern.null(buf)
    assert len(buf) == geo.num_devices()


def test_modulation_sine_square_fourier_radiation() -> None:
    buf = modulation.ModulationBuffer()
    modulation.sine(200.0, modulation.SineOption(), buf)
    assert len(buf) > 0

    sq = modulation.ModulationBuffer()
    modulation.square(150.0, modulation.SquareOption(), sq)
    assert len(sq) > 0

    fo = modulation.ModulationBuffer()
    modulation.fourier(
        [modulation.SineComponent(100.0, modulation.SineOption()),
         modulation.SineComponent(200.0, modulation.SineOption())],
        modulation.FourierOption(),
        fo,
    )
    assert len(fo) > 0

    rp = modulation.ModulationBuffer()
    modulation.sine(200.0, modulation.SineOption(), rp)
    before = len(rp)
    modulation.radiation_pressure(rp)
    assert len(rp) == before


def test_holo_algorithms() -> None:
    geo = geometry()
    wavelength = pattern.wavelength(340_000.0)
    center = geo.center()
    foci = [
        holo.ControlPoint(center + np.array([-20.0, 0.0, 150.0]), holo.Amplitude.pascal(5e3)),
        holo.ControlPoint(center + np.array([20.0, 0.0, 150.0]), holo.Amplitude.spl(150.0)),
    ]
    buf = pattern.PatternBuffer(geo.num_devices())
    holo.naive(geo, foci, wavelength, holo.NaiveOption(), buf)
    holo.gs(geo, foci, wavelength, holo.GsOption(repeat=10), buf)
    holo.gspat(geo, foci, wavelength, holo.GspatOption(repeat=10), buf)
    holo.greedy(geo, foci, wavelength, holo.GreedyOption(), buf)
    assert len(buf) == geo.num_devices()


def test_stm_foci_and_pattern() -> None:
    geo = geometry()
    builder = autd3.DatagramBuilder(geo.num_devices())

    samples = autd3.circle([0.0, 0.0, 150.0], 30.0, 8, [0.0, 0.0, 1.0])
    builder.push(autd3.FociStm(autd3.StmConfig.Freq(1.0), samples, autd3.FociStmOption()))

    wavelength = pattern.wavelength(340_000.0)
    frames = []
    for x in (-20.0, 20.0):
        buf = pattern.PatternBuffer(geo.num_devices())
        pattern.focus(geo, geo.center() + np.array([x, 0.0, 150.0]), wavelength, pattern.FocusOption(), buf)
        frames.append(buf)
    builder.push(autd3.PatternStm(autd3.StmConfig.Freq(1.0), frames, autd3.PatternStmOption()))

    datagrams = builder.build()
    assert len(datagrams) > 0


def test_commands_build() -> None:
    geo = geometry()
    n = geo.num_devices()
    builder = autd3.DatagramBuilder(n)
    builder.push(autd3.Clear())
    builder.push(autd3.Synchronize())
    builder.push(autd3.ForceFan(True))
    builder.push(autd3.SetSilencer(autd3.FixedCompletionTime()))
    builder.push(autd3.SetSilencer(autd3.FixedUpdateRate(intensity=256, phase=256)))
    builder.push(autd3.SetSilencer.disable())
    builder.push(autd3.SetGpioOut([autd3.GpioOut.Off, autd3.GpioOut.BaseSignal,
                                   autd3.GpioOut.PwmOut(0), autd3.GpioOut.Direct(True)]))
    builder.push(autd3.EmulateGpioIn([True, False, True, False]))
    assert len(builder.build()) > 0


def test_pulse_width_table_and_pulse_width() -> None:
    geo = geometry()
    builder = autd3.DatagramBuilder(geo.num_devices())

    table = autd3.PulseWidth.default_table()
    assert len(table) == 256
    assert autd3.SetPulseWidthTable.default_table() == table
    builder.push(autd3.SetPulseWidthTable(table))
    assert len(builder.build()) > 0

    assert autd3.PulseWidth.from_duty(0.5) == 256
    assert autd3.PulseWidth.from_duty(0.0) == 0
    with pytest.raises(ValueError):
        autd3.PulseWidth.from_duty(1.0)
    with pytest.raises(ValueError):
        autd3.PulseWidth.from_duty(-0.5)


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
    import autd3_link_remote as remote
    import autd3_link_twincat as twincat

    remote.RemoteLinkOption("127.0.0.1:8080")
    remote.RemoteLinkOption("127.0.0.1:8080", timeout=0.5)
    twincat.TwinCATLinkOption.local()
    twincat.TwinCATLinkOption.remote("169.254.1.1", "1.2.3.4.1.1", twincat.TwinCATRoute.Ads)


def test_loop_behavior_and_transition_mode() -> None:
    geo = geometry()
    wavelength = pattern.wavelength(340_000.0)
    buf = pattern.PatternBuffer(geo.num_devices())
    pattern.focus(geo, geo.center() + np.array([0.0, 0.0, 150.0]), wavelength, pattern.FocusOption(), buf)

    builder = autd3.DatagramBuilder(geo.num_devices())
    builder.push(
        autd3.Pattern(
            buf,
            loop_behavior=autd3.LoopBehavior.Finite(5),
            transition_mode=autd3.TransitionMode.Gpio(autd3.GpioIn.I1),
        )
    )
    assert len(builder.build()) > 0
