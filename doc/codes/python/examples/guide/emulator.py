import numpy as np

import autd3
import autd3_emulator as emu
import autd3_pattern as pattern
from autd3.units import m, s
from autd3_core import Duration
from autd3.commands import Pattern
from autd3_emulator import Emulator, Recorder, RangeXY, RmsRecordOption, InstantRecordOption

geometry = autd3.geometry.Geometry([autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])
target = geometry.center() + np.array([0.0, 0.0, 150.0])
patterns = geometry.pattern_buffer()
pattern.focus(geometry, target, pattern.wavelength(340 * m / s), pattern.FocusOption(), patterns)

# ANCHOR: record
emulator = Emulator(geometry)


def record_fn(r: Recorder) -> None:
    builder = r.datagram_builder()
    builder.push(Pattern(patterns))
    for frame in builder.build():
        r.send_checked(frame)
    r.tick(Duration.from_millis(1))


record = emulator.record(record_fn)
# ANCHOR_END: record

# ANCHOR: transducer
table = emulator.transducer_table()
print(table)
# ANCHOR_END: transducer

# ANCHOR: drive
phase = record.phase()
print(phase)

pulse_width = record.pulse_width()
print(pulse_width)
# ANCHOR_END: drive

# ANCHOR: output
voltage = record.output_voltage()
print(voltage)

ultrasound = record.output_ultrasound()
print(ultrasound)
# ANCHOR_END: output

rng = RangeXY(
        (target[0] - 20.0, target[0] + 20.0),
        (target[1] - 20.0, target[1] + 20.0),
        float(target[2]),
        1.0,
)
# ANCHOR: points
rms = record.sound_field(rng, RmsRecordOption())
observe_points = rms.observe_points()
print(observe_points)
# ANCHOR_END: points

# ANCHOR: rms
rng = RangeXY(
    x=(target[0] - 20.0, target[0] + 20.0),
    y=(target[1] - 20.0, target[1] + 20.0),
    z=float(target[2]),
    resolution=1.0,
)
rms = record.sound_field(rng, RmsRecordOption())
field = rms.next(Duration.from_micros(25))
print(field)
# ANCHOR_END: rms

# ANCHOR: instant
instant = record.sound_field(
    rng,
    InstantRecordOption(time_step=Duration.from_micros(1)),
)
instant.skip(Duration.from_micros(500))
field = instant.next(Duration.from_micros(25))
print(field)
# ANCHOR_END: instant
