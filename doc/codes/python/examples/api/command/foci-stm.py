import math

from autd3.commands import ActivatePatternBank, ConfigFociStm, FociStm, FociStmOption, WriteFociBuffer
from autd3.geometry import Autd3, Geometry, offset
from autd3.units import Hz, m, mm, s
from autd3.value import ControlPoints, LoopBehavior, PatternBank, SamplingConfig, TransitionMode

geometry = Geometry([Autd3()])
center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm)
radius = 30.0 * mm
num_points = 200
# ANCHOR: circle
points = []
for i in range(num_points):
    theta = 2.0 * math.pi * i / num_points
    p = center + offset(radius * math.cos(theta), radius * math.sin(theta), 0.0 * mm)
    points.append(ControlPoints(p))
# ANCHOR_END: circle
config = SamplingConfig(1.0 * Hz * num_points)
bank = PatternBank.B0
sound_speed = 340 * m / s
loop_behavior = LoopBehavior.Infinite
transition_mode = TransitionMode.Immediate
option = (
    # ANCHOR: option
    FociStmOption(
        bank,
        sound_speed,
        loop_behavior,
        transition_mode,
    )
    # ANCHOR_END: option
)
# ANCHOR: api
FociStm(config, points, option)
# ANCHOR_END: api

num_foci = 1
# ANCHOR: equivalent
max_len = WriteFociBuffer.max_len(num_foci)
for start_index in range(0, len(points), max_len):
    WriteFociBuffer(
        bank=option.bank,
        start_index=start_index,
        points=points[start_index : start_index + max_len],
    )
ConfigFociStm(
    bank=option.bank,
    config=config,
    size=len(points),
    num_foci=num_foci,
    sound_speed=option.sound_speed,
    loop_behavior=option.loop_behavior,
)
ActivatePatternBank(
    bank=option.bank,
    transition_mode=option.transition_mode,
)
# ANCHOR_END: equivalent
