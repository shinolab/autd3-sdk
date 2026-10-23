from autd3.units import Hz
from autd3.value import SamplingConfig
from autd3_modulation import SineOption, modulation_buffer, radiation_pressure_inplace, sine

buffer = modulation_buffer()
sine(150 * Hz, SineOption(), SamplingConfig.FREQ_4K, buffer)

# ANCHOR: api
radiation_pressure_inplace(buffer)
# ANCHOR_END: api
