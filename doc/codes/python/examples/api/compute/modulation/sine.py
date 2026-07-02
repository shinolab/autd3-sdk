from autd3.units import Hz, rad
from autd3.value import Nearest, SamplingConfig
from autd3_modulation import SineOption, modulation_buffer, sine

freq = 150.0 * Hz
option = (
    # ANCHOR: option
    SineOption(
        amplitude=0xFF,
        offset=0x80,
        phase=0.0 * rad,
        clamp=False,
        sampling_config=SamplingConfig.FREQ_4K,
    )
    # ANCHOR_END: option
)
out = modulation_buffer()
# ANCHOR: api
sine(freq, option, out)
# ANCHOR_END: api

# ANCHOR: nearest
sine(Nearest(150.5 * Hz), option, out)
# ANCHOR_END: nearest
