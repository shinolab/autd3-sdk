"""Hardware-free tests: emulator emission recording and sound-field computation."""

from typing import Any

import numpy as np
import polars as pl
import pytest

import autd3
import autd3_emulator as emu
import autd3_modulation as modulation
import autd3_pattern as pattern
from autd3.params import ULTRASOUND_PERIOD
from autd3.units import Hz, m, mm, s
from autd3_core import Duration


def loose(value: object) -> Any:
    return value


def geometry() -> autd3.geometry.Geometry:
    return autd3.geometry.Geometry([autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])


def recorded(geo: autd3.geometry.Geometry) -> emu.Record:
    target = geo.center() + autd3.geometry.offset(0.0 * mm, 0.0 * mm, 150.0 * mm)
    patterns = geo.phase_buffer()
    intensities = geo.intensity_buffer()
    pattern.focus(geo, target, pattern.wavelength(340 * m / s), patterns)

    def record(r: emu.Recorder) -> None:
        r.send(autd3.commands.Pattern(patterns, intensities))
        r.tick(Duration.from_micros(1000))

    return emu.Emulator(geo).record(record)


def test_transducer_table() -> None:
    table = emu.Emulator(geometry()).transducer_table()
    assert isinstance(table, pl.DataFrame)
    assert table.shape == (249, 8)
    assert table.columns == ["dev_idx", "tr_idx", "x[mm]", "y[mm]", "z[mm]", "nx", "ny", "nz"]
    assert table["dev_idx"].dtype == pl.UInt16
    assert table["tr_idx"].dtype == pl.UInt8
    assert table["x[mm]"][1] == pytest.approx(autd3.params.PITCH_MM)
    assert table["nz"][0] == 1.0


def test_record_emission() -> None:
    record = recorded(geometry())
    assert record.num_transducers() == 249
    assert record.num_samples() == 40

    phase = record.phase()
    assert isinstance(phase, pl.DataFrame)
    assert phase.shape == (249, 40)
    assert phase.dtypes[0] == pl.UInt8

    assert record.pulse_width().shape == (249, 40)
    assert record.output_voltage().shape[0] == 249
    assert record.output_ultrasound().shape[0] == 249


def test_record_option_sound_speed_is_velocity() -> None:
    assert emu.RmsRecordOption().sound_speed.m_s == pytest.approx(340.0)
    assert emu.RmsRecordOption(sound_speed=350 * m / s).sound_speed.m_s == pytest.approx(350.0)
    assert emu.InstantRecordOption(sound_speed=350 * m / s).sound_speed.m_s == pytest.approx(350.0)

    option = emu.RmsRecordOption()
    option.sound_speed = 350 * m / s
    assert option.sound_speed.m_s == pytest.approx(350.0)

    with pytest.raises(ValueError):
        emu.RmsRecordOption(sound_speed=loose(340e3))


def test_sound_field_rms() -> None:
    record = recorded(geometry())
    rng = emu.Grid(x=(-10.0, 10.0), y=(-10.0, 10.0), z=150.0, resolution=10.0)
    rms = record.sound_field(rng, emu.RmsRecordOption())

    points = rms.observe_points()
    assert isinstance(points, pl.DataFrame)
    assert points.shape == (9, 3)

    field = rms.next(ULTRASOUND_PERIOD)
    assert isinstance(field, pl.DataFrame)
    assert field.shape == (9, 1)


def test_sound_field_instant() -> None:
    record = recorded(geometry())
    rng = emu.Grid(x=(-10.0, 10.0), y=(-10.0, 10.0), z=150.0, resolution=10.0)
    instant = record.sound_field(rng, emu.InstantRecordOption(time_step=Duration.from_micros(5)))
    instant.skip(Duration.from_micros(500))
    field = instant.next(ULTRASOUND_PERIOD)
    assert isinstance(field, pl.DataFrame)
    assert field.shape == (9, 5)


def test_grid_axes_and_order() -> None:
    record = recorded(geometry())

    def points(grid: emu.Grid) -> list[tuple[float, float, float]]:
        return record.sound_field(grid, emu.RmsRecordOption()).observe_points().rows()

    assert points(emu.Grid(x=(0.0, 1.0), y=(10.0, 11.0), z=150.0, resolution=1.0)) == [
        (0.0, 10.0, 150.0),
        (1.0, 10.0, 150.0),
        (0.0, 11.0, 150.0),
        (1.0, 11.0, 150.0),
    ]
    assert points(emu.Grid(x=(0.0, 1.0), y=(10.0, 11.0), z=150.0, resolution=1.0, order="yxz")) == [
        (0.0, 10.0, 150.0),
        (0.0, 11.0, 150.0),
        (1.0, 10.0, 150.0),
        (1.0, 11.0, 150.0),
    ]
    assert points(emu.Grid(x=3.0, y=4.0, z=(150.0, 152.0), resolution=1.0)) == [
        (3.0, 4.0, 150.0),
        (3.0, 4.0, 151.0),
        (3.0, 4.0, 152.0),
    ]

    with pytest.raises(ValueError):
        emu.Grid(x=0.0, y=0.0, z=150.0, resolution=1.0, order=loose("xy"))
    with pytest.raises(ValueError):
        emu.Grid(x=loose("0"), y=0.0, z=150.0, resolution=1.0)
    with pytest.raises(TypeError):
        record.sound_field(loose((0.0, 0.0, 150.0)), emu.RmsRecordOption())


def test_a_command_copies_its_buffers_when_it_is_created() -> None:
    geo = geometry()
    phases = geo.phase_buffer()
    intensities = geo.intensity_buffer()
    pattern.focus(geo, geo.center() + autd3.geometry.offset(0.0 * mm, 0.0 * mm, 150.0 * mm), pattern.wavelength(340 * m / s), phases)

    def phase_of(command: autd3.commands.Pattern) -> pl.DataFrame:
        def record(r: emu.Recorder) -> None:
            r.send(command)
            r.tick(Duration.from_micros(1000))

        return emu.Emulator(geo).record(record).phase()

    command = autd3.commands.Pattern(phases, intensities)
    expected = phase_of(autd3.commands.Pattern(phases, intensities))
    pattern.add_phase(0x40, phases)
    assert phase_of(command).equals(expected)
    assert not phase_of(autd3.commands.Pattern(phases, intensities)).equals(expected)


def test_a_recorder_sends_commands_and_encoded_frames() -> None:
    geo = geometry()
    phases = geo.phase_buffer()
    intensities = geo.intensity_buffer()
    pattern.focus(geo, geo.center() + autd3.geometry.offset(0.0 * mm, 0.0 * mm, 150.0 * mm), pattern.wavelength(340 * m / s), phases)
    command = autd3.commands.Pattern(phases, intensities)

    def by_command(r: emu.Recorder) -> None:
        r.send([autd3.commands.SetSilencer(), command])
        r.tick(Duration.from_micros(1000))

    def by_frame(r: emu.Recorder) -> None:
        for frame in autd3.Frames.encode(geo, (autd3.commands.SetSilencer(), command)):
            r.send_frame(frame)
        r.tick(Duration.from_micros(1000))

    def by_each(r: emu.Recorder) -> None:
        r.send(autd3.commands.SetSilencer())
        r.send(autd3.commands.each(lambda _: command))
        r.tick(Duration.from_micros(1000))

    expected = emu.Emulator(geo).record(by_command).phase()
    assert emu.Emulator(geo).record(by_frame).phase().equals(expected)
    assert emu.Emulator(geo).record(by_each).phase().equals(expected)


def test_a_recorder_reports_a_frame_the_firmware_rejects() -> None:
    geo = geometry()
    buf = modulation.modulation_buffer()
    modulation.sine(150 * Hz, modulation.SineOption(), buf)
    strict = autd3.commands.SetSilencer(
        autd3.commands.FixedCompletionTime(
            intensity=Duration.from_micros(500),
            phase=Duration.from_micros(1000),
            strict_mode=True,
        )
    )

    def record(r: emu.Recorder) -> None:
        r.send((autd3.commands.Modulation(autd3.value.SamplingConfig.FREQ_4K, buf), strict))

    with pytest.raises(autd3.Autd3Error):
        emu.Emulator(geo).record(record)
