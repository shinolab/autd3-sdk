from autd3.units import Hz
from autd3_modulation import SineOption, modulation_buffer, radiation_pressure, sine

src = modulation_buffer()
sine(150.0 * Hz, SineOption(), src)

dst = modulation_buffer()
# ANCHOR: api
radiation_pressure(src, dst)
# ANCHOR_END: api
