from autd3.units import Hz, rad
from autd3.value import SamplingConfig
from autd3_modulation import SineOption, modulation_buffer, sine

out = modulation_buffer()

sine(
    150 * Hz,
    SineOption(
        amplitude=0xFF,
        offset=0x80,
        phase=0.0 * rad,
        clamp=False,
        sampling_config=SamplingConfig.FREQ_4K,
    ),
    out,
)
