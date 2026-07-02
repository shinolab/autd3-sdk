from autd3.commands import ChangePatternBank, ConfigPattern, PatternCompression, WritePatternBuffer, WritePatternCompressed
from autd3.geometry import Autd3, Geometry
from autd3.value import LoopBehavior, PatternBank, SamplingConfig, TransitionMode

geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

bank = PatternBank.B0
index = 0
emissions = geometry.pattern_buffer()
# ANCHOR: write
WritePatternBuffer(
    bank,
    index,
    emissions,
)
# ANCHOR_END: write
config = SamplingConfig.FREQ_4K
size = 1
loop_behavior = LoopBehavior.Infinite
# ANCHOR: config
ConfigPattern(
    bank,
    config,
    size,
    loop_behavior,
)
# ANCHOR_END: config
transition_mode = TransitionMode.Immediate
# ANCHOR: change
ChangePatternBank(
    bank,
    transition_mode,
)
# ANCHOR_END: change

p0 = geometry.pattern_buffer()
p1 = geometry.pattern_buffer()
p2 = geometry.pattern_buffer()
p3 = geometry.pattern_buffer()
patterns = [p0, p1, p2, p3]
index = 0
format = PatternCompression.PhaseHalf
# ANCHOR: compressed
WritePatternCompressed(
    bank,
    index,
    format,
    patterns,
)
# ANCHOR_END: compressed
