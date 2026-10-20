from autd3.units import Hz
from autd3.value import SamplingConfig
from autd3_modulation import SineOption, modulation_buffer, radiation_pressure, sine

src = modulation_buffer()
sine(150 * Hz, SineOption(), SamplingConfig.FREQ_4K, src)

dst = modulation_buffer()

radiation_pressure(src, dst)
