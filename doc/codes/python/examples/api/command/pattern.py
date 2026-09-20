from autd3.commands import ChangePatternBank, ConfigPattern, Pattern, WritePatternBuffer
from autd3.geometry import Autd3, Geometry
from autd3.value import Intensity, LoopBehavior, PatternBank, SamplingConfig, TransitionMode

geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

bank = PatternBank.B0
transition_mode = TransitionMode.Immediate

phases = geometry.phase_buffer()

# ANCHOR: api
Pattern(phases, Intensity.MAX)

Pattern(
    phases,
    Intensity.MAX,
    bank=bank,
    transition_mode=transition_mode,
)
# ANCHOR_END: api

# ANCHOR: equivalent
WritePatternBuffer(
    bank=bank,
    index=0,
    phases=phases,
    intensities=Intensity.MAX,
)
ConfigPattern(
    bank=bank,
    config=SamplingConfig(0xFFFF),
    size=1,
    loop_behavior=LoopBehavior.Infinite,
)
ChangePatternBank(
    bank=bank,
    transition_mode=transition_mode,
)
# ANCHOR_END: equivalent
