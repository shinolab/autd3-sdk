import math

from autd3.commands import ActivatePatternBank, ConfigFociStm, FociStm, FociStmOption, StmConfig, WriteFociBuffer
from autd3.geometry import Autd3, Geometry, offset
from autd3.units import Hz, m, mm, s
from autd3.value import ControlPoint, ControlPoints, Intensity, LoopBehavior, PatternBank, Phase, TransitionMode

geometry = Geometry([Autd3()])
center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm)
radius = 30.0 * mm
num_points = 200
# ANCHOR: circle
points = []
for i in range(num_points):
    theta = 2.0 * math.pi * i / num_points
    p = center + offset(radius * math.cos(theta), radius * math.sin(theta), 0.0 * mm)
    points.append(ControlPoints([ControlPoint(p, Phase.ZERO)], Intensity.MAX))
# ANCHOR_END: circle
freq = 1.0 * Hz
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
FociStm(freq, points, option)
# ANCHOR_END: api

num_foci = 1
# ANCHOR: equivalent
WriteFociBuffer(
    bank=option.bank,
    start_index=0,
    points=points,
)
ConfigFociStm(
    bank=option.bank,
    config=StmConfig(freq).into_sampling_config(len(points)),
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
