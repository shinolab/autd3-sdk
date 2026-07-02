from autd3.units import Hz
from autd3.value import SamplingConfig
from autd3_modulation import SquareOption, modulation_buffer, square

out = modulation_buffer()

square(
    150 * Hz,
    SquareOption(
        low=0x00,
        high=0xFF,
        duty=0.5,
        sampling_config=SamplingConfig.FREQ_4K,
    ),
    out,
)
