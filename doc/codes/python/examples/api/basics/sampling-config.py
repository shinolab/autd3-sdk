from autd3 import Duration
from autd3.units import kHz
from autd3.value import SamplingConfig

# ANCHOR: api
SamplingConfig(10)
SamplingConfig(4.0 * kHz)
SamplingConfig(Duration.from_micros(250))
# ANCHOR_END: api
