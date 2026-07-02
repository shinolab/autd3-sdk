import asyncio

import numpy as np

import autd3
import autd3_link_ethercrab as ethercrab
import autd3_modulation as modulation
import autd3_pattern as pattern
from autd3.units import Hz, m, s


async def main() -> None:
    geometry = autd3.geometry.Geometry([autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

    client = await autd3.Client.open(
        geometry,
        ethercrab.EtherCrabLinkOption(),
        autd3.ClientConfig(),
    )

    target = geometry.center() + np.array([0.0, 0.0, 150.0])
    wavelength = pattern.wavelength(340 * m / s)

    # ANCHOR: pattern_intensity
    patterns = geometry.pattern_buffer()
    pattern.focus(
        geometry,
        target,
        wavelength,
        pattern.FocusOption(intensity=autd3.value.Intensity(0x80)),
        patterns,
    )
    # ANCHOR_END: pattern_intensity

    # ANCHOR: modulation
    mod_buf = modulation.modulation_buffer()
    modulation.sine(
        200.0 * Hz,
        modulation.SineOption(
            amplitude=0xFF,
            offset=0x80,
            sampling_config=autd3.value.SamplingConfig.FREQ_4K,
        ),
        mod_buf,
    )
    # ANCHOR_END: modulation

    builder = client.datagram_builder()
    builder.push(autd3.commands.SetSilencer())
    builder.push(autd3.commands.Pattern(patterns))
    builder.push(autd3.commands.Modulation(autd3.value.SamplingConfig.FREQ_4K, mod_buf))
    for frame in builder.build():
        await client.send_checked(frame)

    await client.stop()
    await client.close()


if __name__ == "__main__":
    asyncio.run(main())
