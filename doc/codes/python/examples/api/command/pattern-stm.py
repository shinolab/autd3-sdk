import math

from autd3.commands import ActivatePatternBank, ConfigPattern, PatternStm, PatternStmOption, PhaseDepth, WritePatternPhase
from autd3.geometry import Autd3, Geometry, offset
from autd3.units import Hz, m, mm, s
from autd3.value import Intensity, LoopBehavior, PatternBank, SamplingConfig, TransitionMode
from autd3_pattern import focus, wavelength

NUM_POINTS = 200
RADIUS_MM = 30.0

geometry = Geometry([Autd3()])

# Compute the Pattern (emission of all transducers) for each sample point on the host.
center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm)
wavelength = wavelength(340 * m / s)
patterns = []
for i in range(NUM_POINTS):
    theta = 2.0 * math.pi * i / NUM_POINTS
    target = center + offset(RADIUS_MM * math.cos(theta) * mm, RADIUS_MM * math.sin(theta) * mm, 0.0 * mm)
    buffer = geometry.phase_buffer()
    focus(
        geometry,
        target,
        wavelength,
        buffer,
    )
    patterns.append(buffer)
intensities = Intensity.MAX
config = SamplingConfig(1.0 * Hz * NUM_POINTS)
bank = PatternBank.B0
phase_depth = PhaseDepth.Bits8
loop_behavior = LoopBehavior.Infinite
transition_mode = TransitionMode.Immediate
option = (
    # ANCHOR: option
    PatternStmOption(
        bank,
        phase_depth,
        loop_behavior,
        transition_mode,
    )
    # ANCHOR_END: option
)
# ANCHOR: api
PatternStm(config, patterns, intensities, option)
# ANCHOR_END: api

# ANCHOR: equivalent
max_count = option.phase_depth.max_count()
for start_index in range(0, len(patterns), max_count):
    WritePatternPhase(
        bank=option.bank,
        start_index=start_index,
        depth=option.phase_depth,
        intensity=intensities,
        patterns=patterns[start_index : start_index + max_count],
    )
ConfigPattern(
    bank=option.bank,
    config=config,
    size=len(patterns),
    loop_behavior=option.loop_behavior,
)
ActivatePatternBank(
    bank=option.bank,
    transition_mode=option.transition_mode,
)
# ANCHOR_END: equivalent
