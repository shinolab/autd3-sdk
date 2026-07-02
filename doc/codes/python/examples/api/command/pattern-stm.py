import math

import numpy as np

from autd3.commands import ChangePatternBank, ConfigPattern, PatternStm, PatternStmMode, PatternStmOption, StmConfig, WritePatternBuffer
from autd3.geometry import Autd3, Geometry
from autd3.units import Hz, m, s
from autd3.value import LoopBehavior, PatternBank, TransitionMode
from autd3_pattern import FocusOption, focus, wavelength

NUM_POINTS = 200
RADIUS_MM = 30.0

geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

# Compute the Pattern (emission of all transducers) for each sample point on the host.
center = geometry.center() + np.array([0.0, 0.0, 150.0])
wavelength = wavelength(340 * m / s)
patterns = []
for i in range(NUM_POINTS):
    theta = 2.0 * math.pi * i / NUM_POINTS
    target = center + np.array([RADIUS_MM * math.cos(theta), RADIUS_MM * math.sin(theta), 0.0])
    buffer = geometry.pattern_buffer()
    focus(
        geometry,
        target,
        wavelength,
        FocusOption(),
        buffer,
    )
    patterns.append(buffer)
freq = 1.0 * Hz
option = (
    # ANCHOR: option
    PatternStmOption(
        bank=PatternBank.B0,
        mode=PatternStmMode.PhaseIntensityFull,
        loop_behavior=LoopBehavior.Infinite,
        transition_mode=TransitionMode.Immediate,
    )
    # ANCHOR_END: option
)
# ANCHOR: api
PatternStm(freq, patterns, option)
# ANCHOR_END: api

# ANCHOR: equivalent
for index, emissions in enumerate(patterns):
    WritePatternBuffer(
        option.bank,
        index,
        emissions,
    )
ConfigPattern(
    option.bank,
    StmConfig(freq).into_sampling_config(len(patterns)),
    len(patterns),
    loop_behavior=option.loop_behavior,
)
ChangePatternBank(
    option.bank,
    transition_mode=option.transition_mode,
)
# ANCHOR_END: equivalent
