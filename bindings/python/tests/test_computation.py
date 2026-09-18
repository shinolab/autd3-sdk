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
    assert (340 * m / s).mm_s == 340_000.0
    assert (180 * deg).rad == pytest.approx(np.pi)
    assert (np.pi * rad).deg == pytest.approx(180.0)

    assert (2.5 * kPa).as_pascal == pytest.approx(2500.0)


def test_phase_from_angle() -> None:
    assert autd3.value.Phase(np.pi * rad).value == 128
    assert autd3.value.Phase(180 * deg).value == 128
    assert autd3.value.Phase(0x80).value == 0x80
    with pytest.raises(ValueError):
        autd3.value.Phase("invalid")
    assert (2500.0 * Pa) == (2.5 * kPa)
    assert (121.5 * dB).as_pascal == pytest.approx(23.77, abs=1e-2)
    assert (23.77 * Pa).as_spl == pytest.approx(121.5, abs=1e-2)

    with pytest.raises(ValueError):
        pattern.wavelength(340_000.0)
    with pytest.raises(ValueError):
        modulation.sine(200.0, modulation.SineOption(), modulation.modulation_buffer())


def test_pattern_focus_plane_bessel_keep_intensity() -> None:
    geo = geometry()
    wavelength = pattern.wavelength(340 * m / s)
    center = geo.center()
    buf = geo.pattern_buffer()
    for dev in range(len(buf)):
        for tr in range(len(buf[dev])):
            assert (buf[dev][tr].phase.value, buf[dev][tr].intensity.value) == (0x00, 0xFF)

    pattern.set_intensity(autd3.value.Intensity(0x80), buf)
    pattern.focus(geo, center + np.array([0.0, 0.0, 150.0]), wavelength, buf)
    pattern.plane(geo, [0.0, 0.0, 1.0], wavelength, buf)
    pattern.bessel(geo, center, [0.0, 0.0, 1.0], 0.3 * rad, wavelength, buf)
    assert len(buf) == geo.num_devices()
    for dev in range(len(buf)):
        for tr in range(len(buf[dev])):
            assert buf[dev][tr].intensity.value == 0x80


def test_pattern_set_and_add_phase() -> None:
    geo = geometry()
    buf = geo.pattern_buffer()
    pattern.set_intensity(0x80, buf)
    pattern.set_phase(autd3.value.Phase(0xF0), buf)
    pattern.add_phase(0x20, buf)
    for dev in range(len(buf)):
        for tr in range(len(buf[dev])):
            assert (buf[dev][tr].phase.value, buf[dev][tr].intensity.value) == (0x10, 0x80)
    pattern.set_phase_and_intensity(autd3.value.Phase(0x40), autd3.value.Intensity(0x50), buf)
    for dev in range(len(buf)):
        for tr in range(len(buf[dev])):
            assert (buf[dev][tr].phase.value, buf[dev][tr].intensity.value) == (0x40, 0x50)


def test_pattern_group() -> None:
    geo = autd3.geometry.Geometry(
        [
            autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0]),
            autd3.geometry.Autd3([200.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0]),
        ]
    )
    left = geo.pattern_buffer()
    pattern.set_phase_and_intensity(autd3.value.Phase(0x10), autd3.value.Intensity(0x20), left)
    right = geo.pattern_buffer()
    pattern.set_phase_and_intensity(autd3.value.Phase(0x30), autd3.value.Intensity(0x40), right)
    dst = geo.pattern_buffer()

    def key(device: autd3.geometry.Device, tr: int) -> str | None:
        if tr % 3 == 0:
            return "left"
        if device.idx() == 1 and tr % 3 == 1:
            return "right"
        return None

    groups = pattern.TransducerGroups(geo, key)
    assert groups.keys() == ["left", "right"]
    assert groups.key(1, 1) == "right"
    assert groups.key(0, 1) is None

    pattern.group(geo, groups, {"left": left, "right": right}, dst)

    for dev in range(2):
        for tr in range(len(dst[dev])):
            e = dst[dev][tr]
            if tr % 3 == 0:
                expected = (0x10, 0x20)
            elif dev == 1 and tr % 3 == 1:
                expected = (0x30, 0x40)
            else:
                expected = (0x00, 0x00)
            assert (e.phase.value, e.intensity.value) == expected

    with pytest.raises(ValueError):
        pattern.group(geo, groups, {"left": left, "right": dst}, dst)
    with pytest.raises(KeyError):
        pattern.group(geo, groups, {"left": left}, dst)
    single = geometry().pattern_buffer()
    with pytest.raises(ValueError):
        pattern.group(geo, groups, {"left": left, "right": single}, dst)
    with pytest.raises(TypeError):
        pattern.group(geo, groups, {"left": left, "right": 1}, dst)
    with pytest.raises(TypeError):
        pattern.TransducerGroups(geo, lambda device, tr: [])
    with pytest.raises(KeyError):
        groups.mask("center")

    holo_dst = geo.pattern_buffer()
    holo.gspat(
        geo,
        [holo.AmplitudeTarget(point=geo.center() + np.array([0.0, 0.0, 150.0]), amplitude=5e3 * Pa)],
        pattern.wavelength(340 * m / s),
        holo.GspatOption(constraint=holo.EmissionConstraint.Uniform(0xFF), mask=groups.mask("right")),
        holo_dst,
    )
    for dev in range(2):
        for tr in range(len(holo_dst[dev])):
            expected = 0xFF if dev == 1 and tr % 3 == 1 else 0x00
            assert holo_dst[dev][tr].intensity.value == expected

    seen = []

    def compute(side: str, mask: object, buffer: object) -> None:
        seen.append(side)
        if side == "left":
            holo.gspat(
                geo,
                [holo.AmplitudeTarget(point=geo.center() + np.array([0.0, 0.0, 150.0]), amplitude=5e3 * Pa)],
                pattern.wavelength(340 * m / s),
                holo.GspatOption(constraint=holo.EmissionConstraint.Uniform(0xFF), mask=mask),
                buffer,
            )
        else:
            pattern.set_phase_and_intensity(autd3.value.Phase(0x30), autd3.value.Intensity(0x40), buffer)

    computed = geo.pattern_buffer()
    pattern.set_phase_and_intensity(autd3.value.Phase(0xFF), autd3.value.Intensity(0xFF), computed)
    pattern.group_compute(geo, groups, compute, computed)
    assert seen == ["left", "right"]
    for dev in range(2):
        for tr in range(len(computed[dev])):
            e = computed[dev][tr]
            if tr % 3 == 0:
                assert e.intensity.value == 0xFF
            elif dev == 1 and tr % 3 == 1:
                assert (e.phase.value, e.intensity.value) == (0x30, 0x40)
            else:
                assert (e.phase.value, e.intensity.value) == (0x00, 0x00)

    def fail(side: str, mask: object, buffer: object) -> None:
        raise RuntimeError(side)

    with pytest.raises(RuntimeError):
        pattern.group_compute(geo, groups, fail, computed)
    with pytest.raises(ValueError):
        pattern.group_compute(geometry(), groups, compute, single)
    with pytest.raises(TypeError):
        pattern.group_compute(geo, groups, None, computed)

    pattern.group_compute(geo, groups, lambda side, mask, buffer: None, computed)
    for dev in range(2):
        for tr in range(len(computed[dev])):
            e = computed[dev][tr]
            if tr % 3 == 0 or (dev == 1 and tr % 3 == 1):
                assert (e.phase.value, e.intensity.value) == (0x00, 0xFF)
            else:
                assert (e.phase.value, e.intensity.value) == (0x00, 0x00)


def test_pattern_laguerre_hermite_gaussian() -> None:
    geo = geometry()
    wavelength = pattern.wavelength(340 * m / s)
    target = geo.center() + np.array([0.0, 0.0, 150.0])
    num = len(geo.pattern_buffer()[0])

    def phases(buf: object) -> list[int]:
        return [buf[0][i].phase.value for i in range(num)]  # type: ignore[index]

    def intensities(buf: object) -> list[int]:
        return [buf[0][i].intensity.value for i in range(num)]  # type: ignore[index]

    focused = geo.pattern_buffer()
    pattern.focus(geo, target, wavelength, focused)

    def near(a: int, b: int) -> bool:
        return min((a - b) % 256, (b - a) % 256) <= 2

    fundamental = geo.pattern_buffer()
    pattern.laguerre_gaussian_phase(
        geo, target, [0.0, 0.0, 1.0], pattern.LaguerreGaussianOption(p=0, l=0, waist=10.0), wavelength, fundamental
    )
    offsets = [(a - b) % 256 for a, b in zip(phases(fundamental), phases(focused))]
    assert all(near(o, offsets[0]) for o in offsets)

    lg_option = pattern.LaguerreGaussianOption(p=0, l=1, waist=10.0)
    assert (lg_option.p, lg_option.l, lg_option.waist) == (0, 1, 10.0)
    lg = geo.pattern_buffer()
    pattern.set_phase_and_intensity(0x00, 0x00, lg)
    pattern.laguerre_gaussian_phase(geo, target, [0.0, 0.0, 1.0], lg_option, wavelength, lg)
    assert intensities(lg) == [0x00] * num
    assert phases(lg) != phases(fundamental)

    pattern.laguerre_gaussian_intensity(geo, target, [0.0, 0.0, 1.0], lg_option, wavelength, lg)
    assert max(intensities(lg)) == 0xFF
    assert min(intensities(lg)) < 0xFF

    hg_option = pattern.HermiteGaussianOption(m=1, n=0, waist=10.0)
    hg = geo.pattern_buffer()
    pattern.hermite_gaussian_phase(geo, target, [0.0, 0.0, 1.0], [1.0, 0.0, 0.0], hg_option, wavelength, hg)
    pattern.hermite_gaussian_intensity(
        geo, target, [0.0, 0.0, 1.0], [1.0, 0.0, 0.0], hg_option, wavelength, hg
    )
    assert max(intensities(hg)) == 0xFF
    split = [(a - b) % 256 for a, b in zip(phases(hg), phases(focused))]
    assert all(near(o, split[0]) or near(o, split[0] + 128) for o in split)
    assert any(near(o, split[0] + 128) for o in split)

    with pytest.raises(ValueError):
        pattern.LaguerreGaussianOption(p=0, l=1, waist=0.0)
    with pytest.raises(ValueError):
        pattern.HermiteGaussianOption(m=1, n=0, waist=float("nan"))


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

    dst = modulation.modulation_buffer()
    modulation.radiation_pressure(rp, dst)
    assert len(dst) == before
    assert len(rp) == before

    modulation.radiation_pressure_inplace(rp)
    assert len(rp) == before


def test_custom_pattern_indexing() -> None:
    geo = geometry()
    buf = geo.pattern_buffer()

    assert len(buf) == geo.num_devices()
    for slot, device in zip(buf, geo):
        assert len(slot) == device.num_transducers()
        for t in range(len(slot)):
            slot[t] = autd3.value.Emission(autd3.value.Phase(t & 0xFF), autd3.value.Intensity(0x80))

    slot0 = buf[0]
    assert slot0[3].phase.value == 3
    assert slot0[3].intensity.value == 0x80

    with pytest.raises(IndexError):
        _ = buf[geo.num_devices()]
    with pytest.raises(IndexError):
        slot0[len(slot0)] = autd3.value.Emission(autd3.value.Phase(0), autd3.value.Intensity(0))

    autd3.commands.Pattern(buf)


def test_custom_modulation_indexing() -> None:
    buf = modulation.ModulationBuffer(10)
    assert len(buf) == 10
    assert all(buf[i] == 0x00 for i in range(len(buf)))

    buf[0] = 0xFF
    assert buf[0] == 0xFF
    assert list(buf) == [0xFF, *([0x00] * 9)]

    with pytest.raises(IndexError):
        _ = buf[10]
    with pytest.raises(IndexError):
        buf[10] = 0x01

    autd3.commands.Modulation(autd3.value.SamplingConfig.FREQ_4K, buf)


def test_holo_algorithms() -> None:
    geo = geometry()
    wavelength = pattern.wavelength(340 * m / s)
    center = geo.center()
    foci = [
        holo.AmplitudeTarget(center + np.array([-20.0, 0.0, 150.0]), 5e3 * Pa),
        holo.AmplitudeTarget(center + np.array([20.0, 0.0, 150.0]), 150 * dB),
    ]
    buf = geo.pattern_buffer()
    holo.naive(geo, foci, wavelength, holo.NaiveOption(), buf)
    holo.gs(geo, foci, wavelength, holo.GsOption(repeat=10), buf)
    holo.gspat(geo, foci, wavelength, holo.GspatOption(repeat=10), buf)
    holo.greedy(geo, foci, wavelength, holo.GreedyOption(), buf)
    assert len(buf) == geo.num_devices()


def test_stm_foci_and_pattern() -> None:
    geo = geometry()
    builder = autd3.DatagramBuilder(geo)

    samples = []
    autd3.commands.circle([0.0, 0.0, 150.0], 30.0, 8, [0.0, 0.0, 1.0], autd3.value.Intensity.MAX, samples)
    builder.push(autd3.commands.FociStm(1.0 * Hz, samples, autd3.commands.FociStmOption()))

    wavelength = pattern.wavelength(340 * m / s)
    frames = []
    for x in (-20.0, 20.0):
        buf = geo.pattern_buffer()
        pattern.focus(geo, geo.center() + np.array([x, 0.0, 150.0]), wavelength, buf)
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
    pattern.focus(geo, geo.center() + np.array([-40.0, 0.0, 150.0]), wavelength, left)
    right = geo.pattern_buffer()
    pattern.focus(geo, geo.center() + np.array([40.0, 0.0, 150.0]), wavelength, right)
    mod_buf = modulation.modulation_buffer()
    modulation.sine(150 * Hz, modulation.SineOption(), mod_buf)

    # homogeneous per-device command
    builder = autd3.DatagramBuilder(geo)
    builder.push_each(lambda device: autd3.commands.Pattern(left if device.idx() % 2 == 0 else right))
    assert len(builder.build()) > 0

    # heterogeneous per-device command (Python is dynamically typed, no boxing needed)
    builder = autd3.DatagramBuilder(geo)
    builder.push_each(
        lambda device: autd3.commands.Pattern(left)
        if device.idx() % 2 == 0
        else autd3.commands.Modulation(autd3.value.SamplingConfig.FREQ_4K, mod_buf)
    )
    assert len(builder.build()) > 0

    # returning None leaves that device unassigned
    builder = autd3.DatagramBuilder(geo)
    builder.push_each(lambda device: autd3.commands.Pattern(left) if device.idx() == 0 else None)
    assert len(builder.build()) > 0


def test_later_stages_a_bank_without_changing_it() -> None:
    geo = geometry()
    buf = modulation.modulation_buffer()
    modulation.sine(200 * Hz, modulation.SineOption(), buf)

    builder = autd3.DatagramBuilder(geo)
    builder.push(
        autd3.commands.Modulation(
            autd3.value.SamplingConfig.FREQ_4K,
            buf,
            bank=autd3.value.ModulationBank.B1,
            transition_mode=autd3.value.TransitionMode.Later,
        )
    )
    assert len(builder.build()) == 2

    pat = geo.pattern_buffer()
    pattern.set_phase_and_intensity(autd3.value.Phase(0x00), autd3.value.Intensity(0x80), pat)
    builder = autd3.DatagramBuilder(geo)
    builder.push(
        autd3.commands.Pattern(
            pat,
            bank=autd3.value.PatternBank.B1,
            transition_mode=autd3.value.TransitionMode.Later,
        )
    )
    assert len(builder.build()) == 2

    builder = autd3.DatagramBuilder(geo)
    builder.push(
        autd3.commands.ChangeModulationBank(
            autd3.value.ModulationBank.B1,
            transition_mode=autd3.value.TransitionMode.Later,
        )
    )
    with pytest.raises(autd3.Autd3Error, match="Later"):
        builder.build()


def test_commands_build() -> None:
    geo = geometry()
    builder = autd3.DatagramBuilder(geo)
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
    builder = autd3.DatagramBuilder(geo)

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
    pattern.focus(geo, geo.center() + np.array([0.0, 0.0, 150.0]), wavelength, buf)

    builder = autd3.DatagramBuilder(geo)
    builder.push(autd3.commands.WritePatternBuffer(autd3.value.PatternBank.B1, 0, buf))
    builder.push(autd3.commands.WritePatternBuffer(autd3.value.PatternBank.B1, 1, buf))
    builder.push(
        autd3.commands.ConfigPattern(
            autd3.value.PatternBank.B1,
            autd3.value.SamplingConfig.FREQ_4K,
            2,
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
