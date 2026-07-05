from autd3.units import Hz
from autd3.value import SamplingConfig
from autd3_modulation import SquareOption, modulation_buffer, square

freq = 150.0 * Hz
low = 0x00
high = 0xFF
duty = 0.5
sampling_config = SamplingConfig.FREQ_4K
option = (
    # ANCHOR: option
    SquareOption(
        low,
        high,
        duty,
        sampling_config,
    )
    # ANCHOR_END: option
)
dst = modulation_buffer()
# ANCHOR: api
square(freq, option, dst)
# ANCHOR_END: api
