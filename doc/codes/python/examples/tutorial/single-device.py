import asyncio

import numpy as np

import autd3
import autd3_link_ethercrab as ethercrab
import autd3_modulation as modulation
import autd3_pattern as pattern
from autd3.units import Hz, m, s

# xtask:long-running  # [hide]


async def main() -> None:
    # Define a geometry consisting of a single AUTD3 device.
    geometry = autd3.geometry.Geometry([autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

    # Open the client over an EtherCrab link.
    client = await autd3.Client.open(
        geometry,
        ethercrab.EtherCrabLinkOption(),
        autd3.ClientConfig(),
    )

    # Generate a focus 150 mm above the array center.
    target = geometry.center() + np.array([0.0, 0.0, 150.0])
    wavelength = pattern.wavelength(340 * m / s)
    patterns = geometry.pattern_buffer()
    pattern.focus(
        geometry,
        target,
        wavelength,
        pattern.FocusOption(),
        patterns,
    )

    # Apply a 200 Hz sine-wave AM.
    mod_buf = modulation.modulation_buffer()
    modulation.sine(
        200 * Hz,
        modulation.SineOption(sampling_config=autd3.value.SamplingConfig.FREQ_4K),
        mod_buf,
    )

    builder = client.datagram_builder()
    builder.push(autd3.commands.SetSilencer())
    builder.push(autd3.commands.Pattern(patterns))
    builder.push(autd3.commands.Modulation(autd3.value.SamplingConfig.FREQ_4K, mod_buf))
    for frame in builder.build():
        await client.send_checked(frame)

    try:
        await asyncio.Event().wait()
    finally:
        await client.stop()
        await client.close()


if __name__ == "__main__":
    asyncio.run(main())
