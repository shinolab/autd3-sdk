from autd3.units import Hz
from autd3_modulation import (
    FourierOption,
    SineComponent,
    SineOption,
    fourier,
    modulation_buffer,
)

out = modulation_buffer()

fourier(
    [
        SineComponent(
            freq=100 * Hz,
            option=SineOption(),
        )
    ],
    FourierOption(
        scale_factor=None,
        clamp=False,
        offset=0x00,
    ),
    out,
)
