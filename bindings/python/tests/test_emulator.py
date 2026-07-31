"""Hardware-free tests: emulator emission recording and sound-field computation."""

import numpy as np
import polars as pl
import pytest

import autd3
import autd3_emulator as emu
import autd3_pattern as pattern
from autd3.units import m, s
from autd3_core import Duration

ULTRASOUND_PERIOD = Duration.from_micros(25)


def geometry() -> autd3.geometry.Geometry:
    return autd3.geometry.Geometry([autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])


def recorded(geo: autd3.geometry.Geometry) -> emu.Record:
    target = geo.center() + np.array([0.0, 0.0, 150.0])
    patterns = geo.pattern_buffer()
    pattern.focus(geo, target, pattern.wavelength(340 * m / s), pattern.FocusOption(), patterns)

    def record(r: emu.Recorder) -> None:
        builder = r.datagram_builder()
        builder.push(autd3.commands.Pattern(patterns))
        for frame in builder.build():
            r.send_checked(frame)
        r.tick(Duration.from_micros(1000))

    return emu.Emulator(geo).record(record)


def test_transducer_table() -> None:
    table = emu.Emulator(geometry()).transducer_table()
    assert isinstance(table, pl.DataFrame)
    assert table.shape == (249, 8)
    assert table.columns == ["dev_idx", "tr_idx", "x[mm]", "y[mm]", "z[mm]", "nx", "ny", "nz"]
    assert table["dev_idx"].dtype == pl.UInt16
    assert table["tr_idx"].dtype == pl.UInt8
    assert table["x[mm]"][1] == pytest.approx(10.16)
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
        emu.RmsRecordOption(sound_speed=340e3)


def test_sound_field_rms() -> None:
    record = recorded(geometry())
    rng = emu.RangeXY((-10.0, 10.0), (-10.0, 10.0), 150.0, 10.0)
    rms = record.sound_field(rng, emu.RmsRecordOption())

    points = rms.observe_points()
    assert isinstance(points, pl.DataFrame)
    assert points.shape == (9, 3)

    field = rms.next(ULTRASOUND_PERIOD)
    assert isinstance(field, pl.DataFrame)
    assert field.shape == (9, 1)


def test_sound_field_instant() -> None:
    record = recorded(geometry())
    rng = emu.RangeXY((-10.0, 10.0), (-10.0, 10.0), 150.0, 10.0)
    instant = record.sound_field(rng, emu.InstantRecordOption(time_step=Duration.from_micros(5)))
    instant.skip(Duration.from_micros(500))
    field = instant.next(ULTRASOUND_PERIOD)
    assert isinstance(field, pl.DataFrame)
    assert field.shape == (9, 5)
