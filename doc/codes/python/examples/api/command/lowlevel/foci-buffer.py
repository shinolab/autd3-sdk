import math

from autd3.commands import ConfigFociStm, WriteFociBuffer
from autd3.geometry import offset
from autd3.units import Hz, m, mm, s
from autd3.value import ControlPoints, LoopBehavior, PatternBank, SamplingConfig

bank = PatternBank.B0
points = []
for i in range(200):
    theta = 2.0 * math.pi * i / 200
    p = offset(30.0 * mm * math.cos(theta), 30.0 * mm * math.sin(theta), 0.0 * mm)
    points.append(ControlPoints(p))
config = SamplingConfig(1.0 * Hz * len(points))

start_index = 0
# ANCHOR: write
WriteFociBuffer(
    bank=bank,
    start_index=start_index,
    points=points,
)
# ANCHOR_END: write
size = len(points)
num_foci = 1
sound_speed = 340.0 * m / s
loop_behavior = LoopBehavior.Infinite
# ANCHOR: config
ConfigFociStm(
    bank=bank,
    config=config,
    size=size,
    num_foci=num_foci,
    sound_speed=sound_speed,
    loop_behavior=loop_behavior,
)
# ANCHOR_END: config
