from autd3.commands import ConfigFociStm, StmConfig, WriteFociBuffer, circle
from autd3.units import Hz, m, s
from autd3.value import Intensity, LoopBehavior, PatternBank

bank = PatternBank.B0
points = []
circle(
    [0.0, 0.0, 0.0],
    30.0,
    200,
    [0.0, 0.0, 1.0],
    Intensity.MAX,
    points,
)
config = StmConfig(1.0 * Hz).into_sampling_config(len(points))

index_offset = 0
# ANCHOR: write
WriteFociBuffer(
    bank=bank,
    index_offset=index_offset,
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
