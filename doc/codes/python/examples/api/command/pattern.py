from autd3.commands import ChangePatternBank, ConfigPattern, Pattern, WritePatternBuffer
from autd3.geometry import Autd3, Geometry
from autd3.value import LoopBehavior, PatternBank, SamplingConfig, TransitionMode

geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

bank = PatternBank.B0

emissions = geometry.pattern_buffer()

# ANCHOR: api
Pattern(emissions)

Pattern(
    bank=bank,
    emissions=emissions,
)
# ANCHOR_END: api

# ANCHOR: equivalent
WritePatternBuffer(
    bank=bank,
    index=0,
    emissions=emissions,
)
ConfigPattern(
    bank=bank,
    config=SamplingConfig(0xFFFF),
    size=1,
    loop_behavior=LoopBehavior.Infinite,
)
ChangePatternBank(
    bank=bank,
    transition_mode=TransitionMode.Immediate
)
# ANCHOR_END: equivalent
