from autd3.commands import ActivatePatternBank, ConfigFociStm, FociStm, FociStmOption, StmConfig, WriteFociBuffer, circle, line
from autd3.geometry import Autd3, Geometry, offset
from autd3.units import Hz, m, mm, s
from autd3.value import Intensity, LoopBehavior, PatternBank, TransitionMode

geometry = Geometry([Autd3()])
center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm)
radius = 30.0 * mm
num_points = 200
normal = [0.0, 0.0, 1.0]
intensity = Intensity.MAX
dst = []
# ANCHOR: circle
circle(center, radius, num_points, normal, intensity, dst)
# ANCHOR_END: circle

start = center + offset(-15.0 * mm, 0.0 * mm, 0.0 * mm)
end = center + offset(15.0 * mm, 0.0 * mm, 0.0 * mm)
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
ActivatePatternBank(
    bank=option.bank,
    transition_mode=option.transition_mode,
)
# ANCHOR_END: equivalent
