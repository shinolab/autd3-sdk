import numpy as np

from autd3.commands import ChangePatternBank, ConfigFociStm, FociStm, FociStmOption, StmConfig, WriteFociBuffer, circle, line
from autd3.geometry import Autd3, Geometry
from autd3.units import Hz, m, s
from autd3.value import Intensity, LoopBehavior, PatternBank, TransitionMode

geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])
center = geometry.center() + np.array([0.0, 0.0, 150.0])
radius = 30.0
num_points = 200
normal = [0.0, 0.0, 1.0]
intensity = Intensity.MAX
dst = []
# ANCHOR: circle
circle(center, radius, num_points, normal, intensity, dst)
# ANCHOR_END: circle

start = center + np.array([-15.0, 0.0, 0.0])
end = center + np.array([15.0, 0.0, 0.0])
# ANCHOR: line
line(start, end, num_points, intensity, dst)
# ANCHOR_END: line
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
points = dst
# ANCHOR: api
FociStm(freq, points, option)
# ANCHOR_END: api

num_foci = 1
# ANCHOR: equivalent
WriteFociBuffer(
    bank=option.bank,
    index_offset=0,
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
ChangePatternBank(
    bank=option.bank,
    transition_mode=option.transition_mode,
)
# ANCHOR_END: equivalent
