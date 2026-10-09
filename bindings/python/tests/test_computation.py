"""Hardware-free tests: pattern/modulation/holo computation and datagram building."""

from typing import Any

import numpy as np
import pytest

import autd3
import autd3_modulation as modulation
import autd3_pattern as pattern
import autd3_pattern_holo as holo
from autd3.units import Hz, kHz, deg, m, mm, rad, s
from autd3_pattern_holo import Pa, dB, kPa


def loose(value: object) -> Any:
    return value


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

    assert (2.5 * kPa).pascal == pytest.approx(2500.0)


def test_phase_from_angle() -> None:
    assert autd3.value.Phase(np.pi * rad) == autd3.value.Phase(128)
    assert autd3.value.Phase(180 * deg) == autd3.value.Phase(128)
    assert autd3.value.Phase(0x80) == autd3.value.Phase.PI
    with pytest.raises(ValueError):
        autd3.value.Phase(loose("invalid"))
    assert (2500.0 * Pa) == (2.5 * kPa)
    assert (121.5 * dB).pascal == pytest.approx(23.77, abs=1e-2)
    assert (23.77 * Pa).spl == pytest.approx(121.5, abs=1e-2)

    with pytest.raises(ValueError):
        pattern.wavelength(loose(340_000.0))
    with pytest.raises(ValueError):
        modulation.sine(loose(200.0), modulation.SineOption(), modulation.modulation_buffer())


def test_pattern_focus_plane_bessel_write_phase() -> None:
    geo = geometry()
    wavelength = pattern.wavelength(340 * m / s)
    center = geo.center()
    phases = geo.phase_buffer()
    intensities = geo.intensity_buffer()
    for dev in range(len(phases)):
        for tr in range(len(phases[dev])):
            assert phases[dev][tr] == autd3.value.Phase.ZERO
            assert intensities[dev][tr] == autd3.value.Intensity.MAX

    pattern.focus(geo, center + np.array([0.0, 0.0, 150.0]), wavelength, phases)
    focused = [phases[0][tr] for tr in range(len(phases[0]))]
    assert len(set(focused)) > 1
    pattern.plane(geo, [0.0, 0.0, 1.0], wavelength, phases)
    pattern.bessel(geo, center, [0.0, 0.0, 1.0], 0.3 * rad, wavelength, phases)
    assert len(phases) == geo.num_devices()
    with pytest.raises(TypeError):
        pattern.focus(geo, center, wavelength, loose(intensities))


def test_pattern_set_and_add_phase() -> None:
    geo = geometry()
    phases = geo.phase_buffer()
    intensities = geo.intensity_buffer()
    pattern.set_intensity(0x80, intensities)
    pattern.set_phase(autd3.value.Phase(0xF0), phases)
    pattern.add_phase(0x20, phases)
    for dev in range(len(phases)):
        for tr in range(len(phases[dev])):
            assert (phases[dev][tr], intensities[dev][tr]) == (autd3.value.Phase(0x10), autd3.value.Intensity(0x80))
    with pytest.raises(TypeError):
        pattern.set_intensity(0x80, loose(phases))


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
    pattern.set_intensity(0x60, dst_i)

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
            expected = {
                "left": (autd3.value.Phase(0x10), autd3.value.Intensity(0x20)),
                "right": (autd3.value.Phase(0x30), autd3.value.Intensity(0x40)),
                None: (autd3.value.Phase(0xFF), autd3.value.Intensity(0x60)),
            }[side(dev, tr)]
            assert (dst[dev][tr], dst_i[dev][tr]) == expected

    with pytest.raises(ValueError):
        pattern.group(geo, groups, {"left": left, "right": dst}, dst)
    with pytest.raises(KeyError):
        pattern.group(geo, groups, {"left": left}, dst)
    single = geometry().phase_buffer()
    with pytest.raises(ValueError):
        pattern.group(geo, groups, {"left": left, "right": single}, dst)
    with pytest.raises(TypeError):
        pattern.group(geo, groups, loose({"left": left, "right": 1}), dst)
    with pytest.raises(TypeError):
        pattern.group(geo, groups, loose({"left": left, "right": right_i}), dst)
    with pytest.raises(TypeError):
        pattern.group(geo, groups, {"left": left, "right": right}, loose(1))
    with pytest.raises(TypeError):
        pattern.TransducerGroups(geo, loose(lambda device, tr: []))
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
            enabled = autd3.value.Intensity.MAX if side(dev, tr) == "right" else autd3.value.Intensity.MIN
            assert holo_intensities[dev][tr] == enabled

    seen: list[str] = []

    def compute(
        key: str,
        mask: pattern.TransducerMask,
        phases: pattern.PhaseBuffer,
        intensities: pattern.IntensityBuffer,
    ) -> None:
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
    pattern.set_intensity(0x60, computed_i)
    pattern.group_compute(geo, groups, compute, computed, computed_i)
    assert seen == ["left", "right"]
    for dev in range(2):
        for tr in range(len(computed[dev])):
            p, i = computed[dev][tr], computed_i[dev][tr]
            if side(dev, tr) == "left":
                assert i == autd3.value.Intensity.MAX
            elif side(dev, tr) == "right":
                assert (p, i) == (autd3.value.Phase(0x30), autd3.value.Intensity(0x40))
            else:
                assert (p, i) == (autd3.value.Phase(0xFF), autd3.value.Intensity(0x60))

    def fail(
        key: str,
        mask: pattern.TransducerMask,
        phases: pattern.PhaseBuffer,
        intensities: pattern.IntensityBuffer,
    ) -> None:
        raise RuntimeError(key)

    with pytest.raises(RuntimeError):
        pattern.group_compute(geo, groups, fail, computed, computed_i)
    with pytest.raises(ValueError):
        pattern.group_compute(geometry(), groups, compute, single, geometry().intensity_buffer())
    with pytest.raises(TypeError):
        pattern.group_compute(geo, groups, loose(None), computed, computed_i)

    pattern.group_compute(geo, groups, lambda key, mask, phases, intensities: None, computed, computed_i)
    for dev in range(2):
        for tr in range(len(computed[dev])):
            written = (autd3.value.Phase.ZERO, autd3.value.Intensity.MAX)
            untouched = (autd3.value.Phase(0xFF), autd3.value.Intensity(0x60))
            assert (computed[dev][tr], computed_i[dev][tr]) == (untouched if side(dev, tr) is None else written)


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
        geo, target, [0.0, 0.0, 1.0], pattern.LaguerreGaussianOption(p=0, l=0, waist=10.0 * mm), wavelength, fundamental
    )
    offsets = [(a - b) % 256 for a, b in zip(values(fundamental), values(focused))]
    assert all(near(o, offsets[0]) for o in offsets)

    lg_option = pattern.LaguerreGaussianOption(p=0, l=1, waist=10.0 * mm)
    assert (lg_option.p, lg_option.l, lg_option.waist) == (0, 1, 10.0 * mm)
    lg = geo.phase_buffer()
    pattern.laguerre_gaussian_phase(geo, target, [0.0, 0.0, 1.0], lg_option, wavelength, lg)
    assert values(lg) != values(fundamental)

    lg_i = geo.intensity_buffer()
    pattern.set_intensity(0x00, lg_i)
    pattern.laguerre_gaussian_intensity(geo, target, [0.0, 0.0, 1.0], lg_option, wavelength, lg_i)
    assert max(values(lg_i)) == 0xFF
    assert min(values(lg_i)) < 0xFF

    hg_option = pattern.HermiteGaussianOption(m=1, n=0, waist=10.0 * mm)
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
        pattern.LaguerreGaussianOption(p=0, l=1, waist=0.0 * mm)
    with pytest.raises(ValueError):
        pattern.HermiteGaussianOption(m=1, n=0, waist=float("nan") * mm)
    with pytest.raises(ValueError, match="Length"):
        pattern.LaguerreGaussianOption(p=0, l=1, waist=loose(10.0))
    with pytest.raises(ValueError, match="Length"):
        pattern.focus(geo, target, loose(8.5), focused)


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
    assert slot0[3] == autd3.value.Phase(3)
    assert intensities[0][3] == autd3.value.Intensity(0x80)

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
def test_buffer_numpy_round_trip(buffer_type: type[pattern.PhaseBuffer] | type[pattern.IntensityBuffer]) -> None:
    values = (np.arange(2 * 249) & 0xFF).astype(np.uint8).reshape(2, 249)

    buf = buffer_type.from_array(values)
    assert buf.num_devices() == 2
    assert int(buf[1][3]) == values[1, 3]
    out = buf.to_numpy()
    assert out.dtype == np.uint8
    assert out.shape == (2, 249)
    np.testing.assert_array_equal(out, values)

    out[0, 0] = 0xAA
    assert int(buf[0][0]) == values[0, 0]

    reversed_view = values[:, ::-1]
    assert not reversed_view.flags.c_contiguous
    buf.copy_from(reversed_view)
    np.testing.assert_array_equal(buf.to_numpy(), reversed_view)

    empty = buffer_type.from_array(np.zeros((0, 249), dtype=np.uint8))
    assert empty.num_devices() == 0
    assert empty.to_numpy().shape == (0, 249)


@pytest.mark.parametrize("buffer_type", [pattern.PhaseBuffer, pattern.IntensityBuffer])
def test_buffer_numpy_rejects_mismatch(buffer_type: type[pattern.PhaseBuffer] | type[pattern.IntensityBuffer]) -> None:
    buf = buffer_type(2)
    values = np.zeros((2, 249), dtype=np.uint8)

    with pytest.raises(TypeError, match="uint8"):
        buf.copy_from(loose(values.astype(np.int64)))
    with pytest.raises(TypeError, match="uint8"):
        buffer_type.from_array(loose(values.astype(np.float32)))
    with pytest.raises(ValueError, match="shape"):
        buf.copy_from(values[:1])
    with pytest.raises(ValueError, match="shape"):
        buf.copy_from(np.zeros((249, 2), dtype=np.uint8))
    with pytest.raises(ValueError, match=r"expected shape \(n, 249\), got \(249,\)"):
        buffer_type.from_array(np.zeros(249, dtype=np.uint8))
    with pytest.raises(ValueError, match=r"expected shape \(n, 249\)"):
        buffer_type.from_array(np.zeros((2, 249, 1), dtype=np.uint8))
    with pytest.raises(TypeError, match="ndarray"):
        buf.copy_from(loose(values.tolist()))
    with pytest.raises(TypeError, match="masked"):
        buf.copy_from(np.ma.masked_array(values, mask=values > 0))
    with pytest.raises(TypeError):
        buffer_type.from_array(loose("abc"))

    before = buf.to_numpy()
    with pytest.raises(ValueError):
        buf.copy_from(np.full((1, 249), 7, dtype=np.uint8))
    np.testing.assert_array_equal(buf.to_numpy(), before)


def test_buffer_from_array_list_is_unchanged() -> None:
    values = [[autd3.value.Phase(t & 0xFF) for t in range(249)] for _ in range(2)]
    buf = pattern.PhaseBuffer.from_array(values)
    assert buf[1][5] == autd3.value.Phase(5)
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
    phases = (np.rint(-dist / wavelength.mm * 256.0).astype(np.int64) & 0xFF).astype(np.uint8)
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
    commands: list[autd3.commands.Command] = []

    points: list[autd3.value.ControlPoints] = []
    autd3.commands.circle([0.0, 0.0, 150.0], 30.0 * mm, 8, [0.0, 0.0, 1.0], autd3.value.Intensity.MAX, points)
    assert len(points) == 8
    assert all(len(sample.points) == 1 and sample.intensity == autd3.value.Intensity.MAX for sample in points)
    with pytest.raises(ValueError, match="Length"):
        autd3.commands.circle([0.0, 0.0, 150.0], loose(30.0), 8, [0.0, 0.0, 1.0], autd3.value.Intensity.MAX, points)
    commands.append(autd3.commands.FociStm(1.0 * Hz, points, autd3.commands.FociStmOption()))
    commands.append(autd3.commands.FociStm(1.0 * Hz, points=points))

    wavelength = pattern.wavelength(340 * m / s)
    frames = []
    for x in (-20.0, 20.0):
        buf = geo.phase_buffer()
        pattern.focus(geo, geo.center() + np.array([x, 0.0, 150.0]), wavelength, buf)
        frames.append(buf)
    amps = [geo.intensity_buffer() for _ in frames]
    commands.append(
        autd3.commands.PatternStm(autd3.commands.StmConfig(1.0 * Hz), frames, amps, autd3.commands.PatternStmOption())
    )

    datagrams = autd3.Frames.encode(geo, commands)
    assert len(datagrams) > 0


def test_pattern_stm_phase_depth() -> None:
    geo = geometry()
    wavelength = pattern.wavelength(340 * m / s)
    frames = []
    for x in range(12):
        buf = geo.phase_buffer()
        pattern.focus(geo, geo.center() + np.array([float(x), 0.0, 150.0]), wavelength, buf)
        frames.append(buf)

    assert autd3.commands.PhaseDepth.Bits8.max_count() == 5
    assert autd3.commands.PhaseDepth.Bits4.max_count() == 11

    commands: list[autd3.commands.Command] = []
    commands.append(
        autd3.commands.PatternStm(
            autd3.commands.StmConfig(autd3.value.SamplingConfig.FREQ_4K),
            frames,
            autd3.value.Intensity.MAX,
            autd3.commands.PatternStmOption(phase_depth=autd3.commands.PhaseDepth.Bits4),
        )
    )
    assert len(autd3.Frames.encode(geo, commands)) == 4

    amps = [geo.intensity_buffer() for _ in frames]
    commands = []
    commands.append(
        autd3.commands.PatternStm(
            autd3.commands.StmConfig(autd3.value.SamplingConfig.FREQ_4K),
            frames,
            amps,
            autd3.commands.PatternStmOption(phase_depth=autd3.commands.PhaseDepth.Bits4),
        )
    )
    with pytest.raises(autd3.Autd3Error, match="Bits4"):
        autd3.Frames.encode(geo, commands)


def test_each() -> None:
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
    commands: list[autd3.commands.Command] = []
    commands.append(autd3.commands.each(lambda device: autd3.commands.Pattern(left if device.idx() % 2 == 0 else right, amps)))
    assert len(autd3.Frames.encode(geo, commands)) > 0

    # heterogeneous per-device command (Python is dynamically typed, no boxing needed)
    commands = []
    commands.append(autd3.commands.each(
        lambda device: autd3.commands.Pattern(left, amps)
        if device.idx() % 2 == 0
        else autd3.commands.Modulation(autd3.value.SamplingConfig.FREQ_4K, mod_buf)
    ))
    assert len(autd3.Frames.encode(geo, commands)) > 0

    # returning None leaves that device unassigned
    commands = []
    commands.append(autd3.commands.each(lambda device: autd3.commands.Pattern(left, amps) if device.idx() == 0 else None))
    assert len(autd3.Frames.encode(geo, commands)) > 0


def test_later_stages_a_bank_without_changing_it() -> None:
    geo = geometry()
    buf = modulation.modulation_buffer()
    modulation.sine(200 * Hz, modulation.SineOption(), buf)

    commands: list[autd3.commands.Command] = []
    commands.append(
        autd3.commands.Modulation(
            autd3.value.SamplingConfig.FREQ_4K,
            buf,
            bank=autd3.value.ModulationBank.B1,
            transition_mode=autd3.value.TransitionMode.Later,
        )
    )
    assert len(autd3.Frames.encode(geo, commands)) == 2

    pat = geo.phase_buffer()
    pat_i = geo.intensity_buffer()
    pattern.set_intensity(autd3.value.Intensity(0x80), pat_i)
    commands = []
    commands.append(
        autd3.commands.Pattern(
            pat,
            pat_i,
            bank=autd3.value.PatternBank.B1,
            transition_mode=autd3.value.TransitionMode.Later,
        )
    )
    assert len(autd3.Frames.encode(geo, commands)) == 2

    commands = []
    commands.append(
        autd3.commands.ActivateModulationBank(
            autd3.value.ModulationBank.B1,
            transition_mode=autd3.value.TransitionMode.Later,
        )
    )
    with pytest.raises(autd3.Autd3Error, match="Later"):
        autd3.Frames.encode(geo, commands)


def test_commands_build() -> None:
    geo = geometry()
    commands: list[autd3.commands.Command] = []
    commands.append(autd3.commands.Clear())
    commands.append(autd3.commands.Synchronize())
    commands.append(autd3.commands.ReleaseFailsafe())
    commands.append(autd3.commands.ForceFan(True))
    commands.append(autd3.commands.SetSilencer(autd3.commands.FixedCompletionTime()))
    commands.append(autd3.commands.SetSilencer(autd3.commands.FixedUpdateRate(intensity=256, phase=256)))
    commands.append(autd3.commands.SetSilencer.disable())
    commands.append(autd3.commands.SetGpioOut([autd3.commands.GpioOut.Off, autd3.commands.GpioOut.BaseSignal,
                                   autd3.commands.GpioOut.PwmOut(0), autd3.commands.GpioOut.Direct(True)]))
    commands.append(autd3.commands.EmulateGpioIn([True, False, True, False]))
    assert len(autd3.Frames.encode(geo, commands)) > 0


def test_cpu_config_defaults() -> None:
    config = autd3.commands.CpuConfig()
    assert config == autd3.commands.CpuConfig(ptp=autd3.commands.PtpConfig())
    assert config.ptp == autd3.commands.PtpConfig()
    assert config.sys_time_transition_margin.as_nanos() == autd3.Duration.from_millis(10).as_nanos()
    assert config.fpga_wait_update_max_polls == 1_000_000
    assert config.fpga_flash_max_polls == 2_000_000_000
    assert config.sync_guard.as_nanos() == autd3.Duration.from_micros(250).as_nanos()
    assert config.update_activate_delay.as_millis() == 100
    assert config.failsafe_timeout == autd3.Duration.from_millis(500)
    assert config.ptp_unlock_failsafe_timeout is None
    assert config.fpga_bus_wait == autd3.commands.FpgaBusWait.Cycles3
    assert config.fpga_bus_wait.cycles == 3
    assert config.ptp.sync_interval.as_millis() == 8
    assert config.ptp.tx_timestamp_timeout.as_millis() == 3
    assert config.ptp.delay_resp_timeout.as_millis() == 6
    assert config.ptp.holdover.as_millis() == 1000
    assert config.ptp.lock_samples == 64
    assert config.ptp.step_threshold.as_nanos() == 10_000
    assert config.ptp.lock_threshold.as_nanos() == 100
    assert config.ptp.kp_milli == 50
    assert config.ptp.ki_milli == 1
    assert config.ptp.max_freq_ppb == 500_000
    assert config.ptp.delay_req_syncs == 32
    assert config.ptp.path_delay_filter_shift == 5
    assert config.ptp.pause_quanta == 48
    assert config.ptp.pause_hold_syncs == 64
    assert config.ptp.pause_retry.as_millis() == 8
    assert autd3.commands.SetCpuConfig().config == config


def test_cpu_config_fields() -> None:
    ptp = autd3.commands.PtpConfig(
        sync_interval=autd3.Duration.from_millis(14),
        tx_timestamp_timeout=autd3.Duration.from_millis(15),
        delay_resp_timeout=autd3.Duration.from_millis(16),
        holdover=autd3.Duration.from_millis(17),
        lock_samples=18,
        step_threshold=autd3.Duration.from_nanos(19),
        lock_threshold=autd3.Duration.from_nanos(20),
        kp_milli=21,
        ki_milli=22,
        max_freq_ppb=23,
        delay_req_syncs=25,
        path_delay_filter_shift=4,
        pause_quanta=26,
        pause_hold_syncs=27,
        pause_retry=autd3.Duration.from_millis(28),
    )
    config = autd3.commands.CpuConfig(
        sys_time_transition_margin=autd3.Duration.from_nanos(0),
        fpga_wait_update_max_polls=11,
        fpga_flash_max_polls=12,
        sync_guard=autd3.Duration.from_micros(500),
        update_activate_delay=autd3.Duration.from_millis(13),
        failsafe_timeout=autd3.Duration.from_millis(24),
        ptp=ptp,
    )
    assert config != autd3.commands.CpuConfig()
    assert config.sys_time_transition_margin.as_nanos() == 0
    assert config.fpga_wait_update_max_polls == 11
    assert config.fpga_flash_max_polls == 12
    assert config.sync_guard.as_nanos() == 500_000
    assert config.update_activate_delay.as_millis() == 13
    assert config.failsafe_timeout == autd3.Duration.from_millis(24)
    assert config.ptp == ptp
    assert config.ptp.sync_interval.as_millis() == 14
    assert config.ptp.tx_timestamp_timeout.as_millis() == 15
    assert config.ptp.delay_resp_timeout.as_millis() == 16
    assert config.ptp.holdover.as_millis() == 17
    assert config.ptp.lock_samples == 18
    assert config.ptp.step_threshold.as_nanos() == 19
    assert config.ptp.lock_threshold.as_nanos() == 20
    assert config.ptp.kp_milli == 21
    assert config.ptp.ki_milli == 22
    assert config.ptp.max_freq_ppb == 23
    assert config.ptp.delay_req_syncs == 25
    assert config.ptp.path_delay_filter_shift == 4
    assert config.ptp.pause_quanta == 26
    assert config.ptp.pause_hold_syncs == 27
    assert config.ptp.pause_retry.as_millis() == 28
    assert autd3.commands.PtpConfig(pause_quanta=None).pause_quanta is None
    with pytest.raises(ValueError):
        autd3.commands.PtpConfig(delay_req_syncs=0)
    with pytest.raises(ValueError):
        autd3.commands.PtpConfig(pause_quanta=0)

    partial = autd3.commands.CpuConfig(sync_guard=autd3.Duration.from_micros(500))
    assert partial.sync_guard.as_nanos() == 500_000
    assert partial.fpga_wait_update_max_polls == autd3.commands.CpuConfig().fpga_wait_update_max_polls
    assert partial.ptp == autd3.commands.PtpConfig()
    assert partial.failsafe_timeout == autd3.Duration.from_millis(500)

    disabled = autd3.commands.CpuConfig(failsafe_timeout=None)
    assert disabled.failsafe_timeout is None
    assert disabled != autd3.commands.CpuConfig()

    guarded = autd3.commands.CpuConfig(ptp_unlock_failsafe_timeout=autd3.Duration.from_secs(2))
    assert guarded.ptp_unlock_failsafe_timeout == autd3.Duration.from_secs(2)
    assert guarded.failsafe_timeout == autd3.Duration.from_millis(500)
    assert guarded != autd3.commands.CpuConfig()

    fast = autd3.commands.CpuConfig(fpga_bus_wait=autd3.commands.FpgaBusWait.Cycles2)
    assert fast.fpga_bus_wait == autd3.commands.FpgaBusWait.Cycles2
    assert fast.fpga_bus_wait.cycles == 2
    assert fast != autd3.commands.CpuConfig()


def test_cpu_config_rejects_invalid_arguments() -> None:
    with pytest.raises(ValueError, match="fpga_wait_update_max_polls"):
        autd3.commands.CpuConfig(fpga_wait_update_max_polls=0)
    with pytest.raises(ValueError, match="fpga_flash_max_polls"):
        autd3.commands.CpuConfig(fpga_flash_max_polls=0)
    with pytest.raises(ValueError, match="lock_samples"):
        autd3.commands.PtpConfig(lock_samples=0)
    with pytest.raises(TypeError):
        loose(autd3.commands.CpuConfig)(autd3.Duration.from_millis(10))
    with pytest.raises(TypeError):
        autd3.commands.CpuConfig(sync_guard=loose(None))
    with pytest.raises(TypeError):
        autd3.commands.CpuConfig(ptp=loose(None))
    with pytest.raises(TypeError):
        autd3.commands.SetCpuConfig(loose(None))


def test_set_cpu_config_builds() -> None:
    geo = geometry()
    commands: list[autd3.commands.Command] = []
    commands.append(autd3.commands.SetCpuConfig())
    commands.append(autd3.commands.SetCpuConfig(autd3.commands.CpuConfig(sync_guard=autd3.Duration.from_micros(500))))
    commands.append(autd3.commands.SetCpuConfig(autd3.commands.CpuConfig(failsafe_timeout=None)))
    assert len(autd3.Frames.encode(geo, commands)) == 3

    commands = []
    commands.append(autd3.commands.SetCpuConfig(autd3.commands.CpuConfig(failsafe_timeout=autd3.Duration.from_nanos(0))))
    with pytest.raises(autd3.Autd3Error, match="failsafe_timeout"):
        autd3.Frames.encode(geo, commands)

    commands = []
    commands.append(
        autd3.commands.SetCpuConfig(
            autd3.commands.CpuConfig(ptp_unlock_failsafe_timeout=autd3.Duration.from_nanos(0))
        )
    )
    with pytest.raises(autd3.Autd3Error, match="ptp_unlock_failsafe_timeout"):
        autd3.Frames.encode(geo, commands)

    commands = []
    commands.append(autd3.commands.SetCpuConfig(autd3.commands.CpuConfig(update_activate_delay=autd3.Duration.from_micros(1500))))
    with pytest.raises(autd3.Autd3Error, match="update_activate_delay"):
        autd3.Frames.encode(geo, commands)


def test_pulse_width_table_and_pulse_width() -> None:
    geo = geometry()
    commands: list[autd3.commands.Command] = []

    table = autd3.commands.SetPulseWidthTable.empty_table()
    assert table == [autd3.value.PulseWidth(0)] * autd3.params.PWE_TABLE_SIZE
    commands.append(autd3.commands.SetPulseWidthTable(table))
    commands.append(autd3.commands.SetPulseWidthTable())
    assert len(autd3.Frames.encode(geo, commands)) > 0
    with pytest.raises(ValueError):
        autd3.commands.SetPulseWidthTable(table[:-1])
    with pytest.raises(TypeError):
        autd3.commands.SetPulseWidthTable(loose([0] * autd3.params.PWE_TABLE_SIZE))

    half = autd3.value.PulseWidth.from_duty(0.5)
    assert half.pulse_width() == autd3.params.PULSE_WIDTH_PERIOD // 2
    assert autd3.value.PulseWidth.from_duty(0.0).pulse_width() == 0
    assert autd3.value.PulseWidth(12).pulse_width() == 12
    assert autd3.value.PulseWidth(12) == autd3.value.PulseWidth(12)
    assert autd3.value.PulseWidth(12) != autd3.value.PulseWidth(13)
    assert half == autd3.value.PulseWidth.from_duty(0.5)
    assert hash(half) == hash(autd3.value.PulseWidth.from_duty(0.5))
    with pytest.raises(ValueError):
        autd3.value.PulseWidth.from_duty(1.0).pulse_width()
    with pytest.raises(ValueError):
        autd3.value.PulseWidth.from_duty(-0.5).pulse_width()
    with pytest.raises(ValueError):
        autd3.value.PulseWidth(autd3.params.PULSE_WIDTH_PERIOD).pulse_width()

    out_of_range = table.copy()
    out_of_range[0] = autd3.value.PulseWidth(autd3.params.PULSE_WIDTH_PERIOD)
    with pytest.raises(autd3.Autd3Error):
        autd3.Frames.encode(geo, autd3.commands.SetPulseWidthTable(out_of_range))


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

    commands: list[autd3.commands.Command] = []
    commands.append(autd3.commands.WritePatternBuffer(autd3.value.PatternBank.B1, 0, buf, amps))
    commands.append(autd3.commands.WritePatternBuffer(autd3.value.PatternBank.B1, 1, buf, amps))
    commands.append(autd3.commands.WritePatternPhase(autd3.value.PatternBank.B1, 2, autd3.commands.PhaseDepth.Bits8, autd3.value.Intensity.MAX, [buf, buf]))
    commands.append(
        autd3.commands.ConfigPattern(
            autd3.value.PatternBank.B1,
            autd3.value.SamplingConfig.FREQ_4K,
            2,
            loop_behavior=autd3.value.LoopBehavior.Finite(5),
        )
    )
    commands.append(
        autd3.commands.ActivatePatternBank(
            autd3.value.PatternBank.B1,
            transition_mode=autd3.value.TransitionMode.Gpio(autd3.value.GpioIn.I1),
        )
    )
    assert len(autd3.Frames.encode(geo, commands)) > 0


def test_sys_time_feeds_transition_mode_and_gpio() -> None:
    time = autd3.value.SysTime.from_nanos(1_000_000)
    assert time == autd3.value.SysTime.from_nanos(1_000_000)
    assert repr(time) == "SysTime.from_nanos(1000000)"
    later = time + autd3.Duration.from_millis(1)
    assert later == autd3.value.SysTime.from_nanos(2_000_000)
    assert later - time == autd3.Duration.from_millis(1)
    assert time - later == autd3.Duration.from_nanos(0)
    assert later - autd3.Duration.from_millis(1) == time
    assert time < later
    autd3.value.TransitionMode.SysTime(time)
    with pytest.raises(TypeError):
        loose(autd3.value.TransitionMode.SysTime)(time, autd3.Duration.from_millis(1))
    with pytest.raises(TypeError):
        loose(autd3.value.TransitionMode.SysTime)(time, margin=autd3.Duration.from_millis(1))
    autd3.commands.GpioOut.SysTimeEq(time)


def test_zero_divider_is_rejected_when_the_command_is_created() -> None:
    class ZeroDivider:
        def divide(self) -> int:
            return 0

    buf = modulation.modulation_buffer()
    modulation.sine(150 * Hz, modulation.SineOption(), buf)
    with pytest.raises(ValueError, match="divider must be >= 1"):
        autd3.commands.Modulation(loose(ZeroDivider()), buf)
