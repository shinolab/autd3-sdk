from autd3.commands import ChangePatternBank, ConfigPattern, PatternCompression, WritePatternBuffer, WritePatternCompressed
from autd3.geometry import Autd3, Geometry
from autd3.value import Intensity, LoopBehavior, PatternBank, SamplingConfig, TransitionMode

geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

bank = PatternBank.B0
index = 0
phases = geometry.phase_buffer()
intensities = Intensity.MAX
# ANCHOR: write
WritePatternBuffer(
    bank=bank,
    index=index,
    phases=phases,
    intensities=intensities,
)
# ANCHOR_END: write
config = SamplingConfig.FREQ_4K
size = 1
loop_behavior = LoopBehavior.Infinite
# ANCHOR: config
ConfigPattern(
    bank=bank,
    config=config,
    size=size,
    loop_behavior=loop_behavior,
)
# ANCHOR_END: config
transition_mode = TransitionMode.Immediate
# ANCHOR: change
ChangePatternBank(
    bank=bank,
    transition_mode=transition_mode,
)
# ANCHOR_END: change

p0 = geometry.phase_buffer()
p1 = geometry.phase_buffer()
p2 = geometry.phase_buffer()
p3 = geometry.phase_buffer()
patterns = [p0, p1, p2, p3]
index = 0
format = PatternCompression.PhaseHalf
intensity = Intensity.MAX
# ANCHOR: compressed
WritePatternCompressed(
    bank=bank,
    index=index,
    format=format,
    intensity=intensity,
    patterns=patterns,
)
# ANCHOR_END: compressed
