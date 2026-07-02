from autd3.units import Hz
from autd3_modulation import (
    FourierOption,
    SineComponent,
    SineOption,
    fourier,
    modulation_buffer,
)

option = (
    # ANCHOR: option
    FourierOption(
        scale_factor=None,
        clamp=False,
        offset=0x00,
    )
    # ANCHOR_END: option
)
out = modulation_buffer()

# Shown standalone in the SineComponent section of the docs.
# ANCHOR: components
SineComponent(
    100.0 * Hz,
    SineOption(),
)
# ANCHOR_END: components

components = [
    SineComponent(
        100.0 * Hz,
        SineOption(),
    )
]
# ANCHOR: api
fourier(components, option, out)
# ANCHOR_END: api
