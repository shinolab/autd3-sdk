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


def test_pattern_focus_plane_bessel_write_phase() -> None:
    geo = geometry()
    wavelength = pattern.wavelength(340 * m / s)
    center = geo.center()
    phases = geo.phase_buffer()
    intensities = geo.intensity_buffer()
    for dev in range(len(phases)):
        for tr in range(len(phases[dev])):
            assert phases[dev][tr].value == 0x00
            assert intensities[dev][tr].value == 0xFF

    pattern.focus(geo, center + np.array([0.0, 0.0, 150.0]), wavelength, phases)
    focused = [phases[0][tr].value for tr in range(len(phases[0]))]
    assert len(set(focused)) > 1
    pattern.plane(geo, [0.0, 0.0, 1.0], wavelength, phases)
    pattern.bessel(geo, center, [0.0, 0.0, 1.0], 0.3 * rad, wavelength, phases)
    assert len(phases) == geo.num_devices()
    with pytest.raises(TypeError):
        pattern.focus(geo, center, wavelength, intensities)


def test_pattern_set_and_add_phase() -> None:
    geo = geometry()
    phases = geo.phase_buffer()
    intensities = geo.intensity_buffer()
    pattern.set_intensity(0x80, intensities)
    pattern.set_phase(autd3.value.Phase(0xF0), phases)
    pattern.add_phase(0x20, phases)
    for dev in range(len(phases)):
        for tr in range(len(phases[dev])):
            assert (phases[dev][tr].value, intensities[dev][tr].value) == (0x10, 0x80)
    assert not hasattr(pattern, "set_phase_and_intensity")
    with pytest.raises(TypeError):
        pattern.set_intensity(0x80, phases)


def test_pattern_group() -> None:
    geo = autd3.geometry.Geometry(
        [
            autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0]),
            autd3.geometry.Autd3([200.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0]),
        ]
    )
    left = geo.phase_buffer()
    pattern.set_phase(autd3.value.Phase(0x10), left)
    right = geo.phase_buffer()
    pattern.set_phase(autd3.value.Phase(0x30), right)
    dst = geo.phase_buffer()
    pattern.set_phase(autd3.value.Phase(0xFF), dst)

    left_i = geo.intensity_buffer()
    pattern.set_intensity(0x20, left_i)
    right_i = geo.intensity_buffer()
    pattern.set_intensity(0x40, right_i)
    dst_i = geo.intensity_buffer()

    def key(device: autd3.geometry.Device, tr: int) -> str | None:
        if tr % 3 == 0:
            return "left"
        if device.idx() == 1 and tr % 3 == 1:
            return "right"
        return None

    def side(dev: int, tr: int) -> str | None:
        if tr % 3 == 0:
            return "left"
        if dev == 1 and tr % 3 == 1:
            return "right"
        return None

    groups = pattern.TransducerGroups(geo, key)
    assert groups.keys() == ["left", "right"]
    assert groups.key(1, 1) == "right"
    assert groups.key(0, 1) is None

    pattern.group(geo, groups, {"left": left, "right": right}, dst)
    pattern.group(geo, groups, {"left": left_i, "right": right_i}, dst_i)

    for dev in range(2):
        for tr in range(len(dst[dev])):
            expected = {"left": (0x10, 0x20), "right": (0x30, 0x40), None: (0x00, 0x00)}[side(dev, tr)]
            assert (dst[dev][tr].value, dst_i[dev][tr].value) == expected

    with pytest.raises(ValueError):
        pattern.group(geo, groups, {"left": left, "right": dst}, dst)
    with pytest.raises(KeyError):
        pattern.group(geo, groups, {"left": left}, dst)
    single = geometry().phase_buffer()
    with pytest.raises(ValueError):
        pattern.group(geo, groups, {"left": left, "right": single}, dst)
    with pytest.raises(TypeError):
        pattern.group(geo, groups, {"left": left, "right": 1}, dst)
    with pytest.raises(TypeError):
        pattern.group(geo, groups, {"left": left, "right": right_i}, dst)
    with pytest.raises(TypeError):
        pattern.group(geo, groups, {"left": left, "right": right}, 1)
    with pytest.raises(TypeError):
        pattern.TransducerGroups(geo, lambda device, tr: [])
    with pytest.raises(KeyError):
        groups.mask("center")

    holo_phases = geo.phase_buffer()
    holo_intensities = geo.intensity_buffer()
    holo.gspat(
        geo,
        [holo.AmplitudeTarget(point=geo.center() + np.array([0.0, 0.0, 150.0]), amplitude=5e3 * Pa)],
        pattern.wavelength(340 * m / s),
        holo.GspatOption(constraint=holo.IntensityConstraint.Uniform(0xFF), mask=groups.mask("right")),
        holo_phases,
        holo_intensities,
    )
    for dev in range(2):
        for tr in range(len(holo_intensities[dev])):
            expected = 0xFF if side(dev, tr) == "right" else 0x00
            assert holo_intensities[dev][tr].value == expected

    seen = []

    def compute(key: str, mask: object, phases: object, intensities: object) -> None:
        seen.append(key)
        if key == "left":
            holo.gspat(
                geo,
                [holo.AmplitudeTarget(point=geo.center() + np.array([0.0, 0.0, 150.0]), amplitude=5e3 * Pa)],
                pattern.wavelength(340 * m / s),
                holo.GspatOption(constraint=holo.IntensityConstraint.Uniform(0xFF), mask=mask),
                phases,
                intensities,
            )
        else:
            pattern.set_phase(autd3.value.Phase(0x30), phases)
            pattern.set_intensity(autd3.value.Intensity(0x40), intensities)

    computed = geo.phase_buffer()
    pattern.set_phase(autd3.value.Phase(0xFF), computed)
    computed_i = geo.intensity_buffer()
    pattern.group_compute(geo, groups, compute, computed, computed_i)
    assert seen == ["left", "right"]
    for dev in range(2):
        for tr in range(len(computed[dev])):
            p, i = computed[dev][tr].value, computed_i[dev][tr].value
            if side(dev, tr) == "left":
                assert i == 0xFF
            elif side(dev, tr) == "right":
                assert (p, i) == (0x30, 0x40)
            else:
                assert (p, i) == (0x00, 0x00)

    def fail(key: str, mask: object, phases: object, intensities: object) -> None:
        raise RuntimeError(key)

    with pytest.raises(RuntimeError):
        pattern.group_compute(geo, groups, fail, computed, computed_i)
    with pytest.raises(ValueError):
        pattern.group_compute(geometry(), groups, compute, single, geometry().intensity_buffer())
    with pytest.raises(TypeError):
        pattern.group_compute(geo, groups, None, computed, computed_i)

    pattern.group_compute(geo, groups, lambda key, mask, phases, intensities: None, computed, computed_i)
    for dev in range(2):
        for tr in range(len(computed[dev])):
            p, i = computed[dev][tr].value, computed_i[dev][tr].value
            if side(dev, tr) is not None:
                assert (p, i) == (0x00, 0xFF)
            else:
                assert (p, i) == (0x00, 0x00)


def test_pattern_laguerre_hermite_gaussian() -> None:
    geo = geometry()
    wavelength = pattern.wavelength(340 * m / s)
    target = geo.center() + np.array([0.0, 0.0, 150.0])
    num = len(geo.phase_buffer()[0])

    def values(buf: object) -> list[int]:
        return [buf[0][i].value for i in range(num)]  # type: ignore[index]

    focused = geo.phase_buffer()
    pattern.focus(geo, target, wavelength, focused)

    def near(a: int, b: int) -> bool:
        return min((a - b) % 256, (b - a) % 256) <= 2

    fundamental = geo.phase_buffer()
    pattern.laguerre_gaussian_phase(
        geo, target, [0.0, 0.0, 1.0], pattern.LaguerreGaussianOption(p=0, l=0, waist=10.0), wavelength, fundamental
    )
    offsets = [(a - b) % 256 for a, b in zip(values(fundamental), values(focused))]
    assert all(near(o, offsets[0]) for o in offsets)

    lg_option = pattern.LaguerreGaussianOption(p=0, l=1, waist=10.0)
    assert (lg_option.p, lg_option.l, lg_option.waist) == (0, 1, 10.0)
    lg = geo.phase_buffer()
    pattern.laguerre_gaussian_phase(geo, target, [0.0, 0.0, 1.0], lg_option, wavelength, lg)
    assert values(lg) != values(fundamental)

    lg_i = geo.intensity_buffer()
    pattern.set_intensity(0x00, lg_i)
    pattern.laguerre_gaussian_intensity(geo, target, [0.0, 0.0, 1.0], lg_option, wavelength, lg_i)
    assert max(values(lg_i)) == 0xFF
    assert min(values(lg_i)) < 0xFF

    hg_option = pattern.HermiteGaussianOption(m=1, n=0, waist=10.0)
    hg = geo.phase_buffer()
    hg_i = geo.intensity_buffer()
    pattern.hermite_gaussian_phase(geo, target, [0.0, 0.0, 1.0], [1.0, 0.0, 0.0], hg_option, wavelength, hg)
    pattern.hermite_gaussian_intensity(
        geo, target, [0.0, 0.0, 1.0], [1.0, 0.0, 0.0], hg_option, wavelength, hg_i
    )
    assert max(values(hg_i)) == 0xFF
    split = [(a - b) % 256 for a, b in zip(values(hg), values(focused))]
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
    phases = geo.phase_buffer()
    intensities = geo.intensity_buffer()

    assert len(phases) == geo.num_devices()
    assert len(intensities) == geo.num_devices()
    for slot, islot, device in zip(phases, intensities, geo):
        assert len(slot) == device.num_transducers()
        for t in range(len(slot)):
            slot[t] = autd3.value.Phase(t & 0xFF)
            islot[t] = autd3.value.Intensity(0x80)

    slot0 = phases[0]
    assert slot0[3].value == 3
    assert intensities[0][3].value == 0x80

    with pytest.raises(IndexError):
        _ = phases[geo.num_devices()]
    with pytest.raises(IndexError):
        slot0[len(slot0)] = autd3.value.Phase(0)

    autd3.commands.Pattern(phases, intensities)


def two_devices() -> autd3.geometry.Geometry:
    return autd3.geometry.Geometry(
        [
            autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0]),
            autd3.geometry.Autd3([192.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0]),
        ]
    )


@pytest.mark.parametrize("buffer_type", [pattern.PhaseBuffer, pattern.IntensityBuffer])
def test_buffer_numpy_round_trip(buffer_type: type) -> None:
    values = (np.arange(2 * 249) & 0xFF).astype(np.uint8).reshape(2, 249)

    buf = buffer_type.from_array(values)
    assert buf.num_devices() == 2
    assert buf[1][3].value == values[1, 3]
    out = buf.to_numpy()
    assert out.dtype == np.uint8
    assert out.shape == (2, 249)
    np.testing.assert_array_equal(out, values)

    out[0, 0] = 0xAA
    assert buf[0][0].value == values[0, 0]

    reversed_view = values[:, ::-1]
    assert not reversed_view.flags.c_contiguous
    buf.copy_from(reversed_view)
    np.testing.assert_array_equal(buf.to_numpy(), reversed_view)

    empty = buffer_type.from_array(np.zeros((0, 249), dtype=np.uint8))
    assert empty.num_devices() == 0
    assert empty.to_numpy().shape == (0, 249)


@pytest.mark.parametrize("buffer_type", [pattern.PhaseBuffer, pattern.IntensityBuffer])
def test_buffer_numpy_rejects_mismatch(buffer_type: type) -> None:
    buf = buffer_type(2)
    values = np.zeros((2, 249), dtype=np.uint8)

    with pytest.raises(TypeError, match="uint8"):
        buf.copy_from(values.astype(np.int64))
    with pytest.raises(TypeError, match="uint8"):
        buffer_type.from_array(values.astype(np.float32))
    with pytest.raises(ValueError, match="shape"):
        buf.copy_from(values[:1])
    with pytest.raises(ValueError, match="shape"):
        buf.copy_from(np.zeros((249, 2), dtype=np.uint8))
    with pytest.raises(ValueError, match=r"expected shape \(n, 249\), got \(249,\)"):
        buffer_type.from_array(np.zeros(249, dtype=np.uint8))
    with pytest.raises(ValueError, match=r"expected shape \(n, 249\)"):
        buffer_type.from_array(np.zeros((2, 249, 1), dtype=np.uint8))
    with pytest.raises(TypeError, match="ndarray"):
        buf.copy_from(values.tolist())
    with pytest.raises(TypeError, match="masked"):
        buf.copy_from(np.ma.masked_array(values, mask=values > 0))
    with pytest.raises(TypeError):
        buffer_type.from_array("abc")

    before = buf.to_numpy()
    with pytest.raises(ValueError):
        buf.copy_from(np.full((1, 249), 7, dtype=np.uint8))
    np.testing.assert_array_equal(buf.to_numpy(), before)


def test_buffer_from_array_list_is_unchanged() -> None:
    values = [[autd3.value.Phase(t & 0xFF) for t in range(249)] for _ in range(2)]
    buf = pattern.PhaseBuffer.from_array(values)
    assert buf[1][5].value == 5
    with pytest.raises(ValueError):
        pattern.PhaseBuffer.from_array([[0] * 248])


def test_positions_and_directions_are_float32_arrays() -> None:
    geo = autd3.geometry.Geometry(
        [autd3.geometry.Autd3([10.0, 20.0, 30.0], [np.cos(np.pi / 4), 0.0, np.sin(np.pi / 4), 0.0])]
    )
    dev = geo[0]
    positions = dev.positions()
    directions = dev.directions()
    assert positions.dtype == np.float32
    assert directions.dtype == np.float32
    assert positions.shape == (dev.num_transducers(), 3)
    assert directions.shape == (dev.num_transducers(), 3)
    for t in (0, 17, dev.num_transducers() - 1):
        np.testing.assert_allclose(positions[t], dev.position(t), atol=1e-4)
        np.testing.assert_allclose(directions[t], dev.direction(t), atol=1e-6)
    np.testing.assert_allclose(directions[0], [1.0, 0.0, 0.0], atol=1e-6)


def test_numpy_focus_matches_native_focus() -> None:
    geo = two_devices()
    wavelength = pattern.wavelength(340 * m / s)
    target = geo.center() + np.array([10.0, -20.0, 150.0])

    native = geo.phase_buffer()
    pattern.focus(geo, target, wavelength, native)

    positions = np.stack([device.positions() for device in geo])
    dist = np.linalg.norm(positions - target.astype(np.float32), axis=2)
    phases = (np.rint(-dist / wavelength * 256.0).astype(np.int64) & 0xFF).astype(np.uint8)
    custom = geo.phase_buffer()
    custom.copy_from(phases)

    diff = (custom.to_numpy().astype(np.int16) - native.to_numpy().astype(np.int16)) & 0xFF
    assert np.all((diff == 0) | (diff == 1) | (diff == 0xFF))


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
    phases = geo.phase_buffer()
    intensities = geo.intensity_buffer()
    holo.naive(geo, foci, wavelength, holo.NaiveOption(), phases, intensities)
    holo.gs(geo, foci, wavelength, holo.GsOption(repeat=10), phases, intensities)
    holo.gspat(geo, foci, wavelength, holo.GspatOption(repeat=10), phases, intensities)
    holo.greedy(geo, foci, wavelength, holo.GreedyOption(), phases, intensities)
    assert len(phases) == geo.num_devices()
    assert len(intensities) == geo.num_devices()


def test_stm_foci_and_pattern() -> None:
    geo = geometry()
    builder = autd3.DatagramBuilder(geo)

    samples = []
    autd3.commands.circle([0.0, 0.0, 150.0], 30.0, 8, [0.0, 0.0, 1.0], autd3.value.Intensity.MAX, samples)
    builder.push(autd3.commands.FociStm(1.0 * Hz, samples, autd3.commands.FociStmOption()))

    wavelength = pattern.wavelength(340 * m / s)
    frames = []
    for x in (-20.0, 20.0):
        buf = geo.phase_buffer()
        pattern.focus(geo, geo.center() + np.array([x, 0.0, 150.0]), wavelength, buf)
        frames.append(buf)
    amps = [geo.intensity_buffer() for _ in frames]
    builder.push(
        autd3.commands.PatternStm(autd3.commands.StmConfig(1.0 * Hz), frames, amps, autd3.commands.PatternStmOption())
    )

    datagrams = builder.build()
    assert len(datagrams) > 0


def test_push_each() -> None:
    geo = autd3.geometry.Geometry([
        autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0]),
        autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0]),
    ])
    wavelength = pattern.wavelength(340 * m / s)
    left = geo.phase_buffer()
    pattern.focus(geo, geo.center() + np.array([-40.0, 0.0, 150.0]), wavelength, left)
    right = geo.phase_buffer()
    pattern.focus(geo, geo.center() + np.array([40.0, 0.0, 150.0]), wavelength, right)
    amps = geo.intensity_buffer()
    mod_buf = modulation.modulation_buffer()
    modulation.sine(150 * Hz, modulation.SineOption(), mod_buf)

    # homogeneous per-device command
    builder = autd3.DatagramBuilder(geo)
    builder.push_each(lambda device: autd3.commands.Pattern(left if device.idx() % 2 == 0 else right, amps))
    assert len(builder.build()) > 0

    # heterogeneous per-device command (Python is dynamically typed, no boxing needed)
    builder = autd3.DatagramBuilder(geo)
    builder.push_each(
        lambda device: autd3.commands.Pattern(left, amps)
        if device.idx() % 2 == 0
        else autd3.commands.Modulation(autd3.value.SamplingConfig.FREQ_4K, mod_buf)
    )
    assert len(builder.build()) > 0

    # returning None leaves that device unassigned
    builder = autd3.DatagramBuilder(geo)
    builder.push_each(lambda device: autd3.commands.Pattern(left, amps) if device.idx() == 0 else None)
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

    pat = geo.phase_buffer()
    pat_i = geo.intensity_buffer()
    pattern.set_intensity(autd3.value.Intensity(0x80), pat_i)
    builder = autd3.DatagramBuilder(geo)
    builder.push(
        autd3.commands.Pattern(
            pat,
            pat_i,
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


def test_transport_options_construct() -> None:
    autd3.TransportOption()
    autd3.TransportOption(iface="eth0", heartbeat=autd3.Duration.from_millis(2))


def test_loop_behavior_and_transition_mode() -> None:
    geo = geometry()
    wavelength = pattern.wavelength(340 * m / s)
    buf = geo.phase_buffer()
    amps = geo.intensity_buffer()
    pattern.focus(geo, geo.center() + np.array([0.0, 0.0, 150.0]), wavelength, buf)

    builder = autd3.DatagramBuilder(geo)
    builder.push(autd3.commands.WritePatternBuffer(autd3.value.PatternBank.B1, 0, buf, amps))
    builder.push(autd3.commands.WritePatternBuffer(autd3.value.PatternBank.B1, 1, buf, amps))
    builder.push(autd3.commands.WritePatternCompressed(autd3.value.PatternBank.B1, 2, autd3.commands.PatternCompression.PhaseFull, autd3.value.Intensity.MAX, [buf, buf]))
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


def test_sys_time_feeds_transition_mode_and_gpio() -> None:
    time = autd3.value.SysTime.from_nanos(1_000_000)
    assert time.sys_time == 1_000_000
    assert repr(time) == "SysTime.from_nanos(1000000)"
    assert (time + autd3.Duration.from_millis(1)).sys_time == 2_000_000
    autd3.value.TransitionMode.SysTime(time)
    autd3.commands.GpioOut.SysTimeEq(time)
