from autd3.units import Hz
from autd3.value import SamplingConfig
from autd3_modulation import SquareOption, modulation_buffer, square

freq = 150 * Hz
low = 0x00
high = 0xFF
duty = 0.5
option = (
    # ANCHOR: option
    SquareOption(
        low,
        high,
        duty,
    )
    # ANCHOR_END: option
)
sampling_config = SamplingConfig.FREQ_4K
dst = modulation_buffer()
# ANCHOR: api
square(freq, option, sampling_config, dst)
# ANCHOR_END: api
