import asyncio

import numpy as np

import autd3_link_ethercrab as ethercrab
import autd3_modulation as modulation
import autd3_pattern as pattern
from autd3 import Client, ClientConfig
from autd3.commands import Modulation, Pattern, SetSilencer
from autd3.geometry import Autd3, Geometry
from autd3.units import Hz, m, s
from autd3.value import Intensity, SamplingConfig

async def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

    client = await Client.open(
        geometry,
        ethercrab.EtherCrabLinkOption(),
        ClientConfig(),
    )

    target = geometry.center() + np.array([0.0, 0.0, 150.0])
    wavelength = pattern.wavelength(340 * m / s)

    # ANCHOR: pattern_intensity
    patterns = geometry.pattern_buffer()
    pattern.focus(
        geometry,
        target,
        wavelength,
        pattern.FocusOption(intensity=Intensity(0x80)),
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
            sampling_config=SamplingConfig.FREQ_4K,
        ),
        mod_buf,
    )
    # ANCHOR_END: modulation

    builder = client.datagram_builder()
    builder.push(SetSilencer())
    builder.push(Pattern(patterns))
    builder.push(Modulation(SamplingConfig.FREQ_4K, mod_buf))
    for frame in builder.build():
        await client.send_checked(frame)

    await client.stop()
    await client.close()


if __name__ == "__main__":
    asyncio.run(main())
