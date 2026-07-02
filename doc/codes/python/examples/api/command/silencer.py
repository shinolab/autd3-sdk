from autd3_core import Duration

from autd3.commands import FixedCompletionTime, FixedUpdateRate, SetSilencer

intensity = Duration.from_micros(250)
phase = Duration.from_micros(1000)
strict_mode = True
# ANCHOR: api
SetSilencer()

SetSilencer.disable()

SetSilencer(
    FixedCompletionTime(
        intensity,
        phase,
        strict_mode,
    )
)
# ANCHOR_END: api

intensity = 256
phase = 256

# ANCHOR: api
SetSilencer(FixedUpdateRate(intensity, phase))
# ANCHOR_END: api
