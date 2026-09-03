import asyncio

import numpy as np

import autd3_link_echocat as echocat
import autd3_modulation as modulation
import autd3_pattern as pattern
from autd3 import Client, ClientConfig
from autd3.commands import Modulation, Pattern, SetSilencer
from autd3.geometry import Autd3, Geometry
from autd3.units import Hz, m, s
from autd3.value import SamplingConfig

# xtask:long-running  # [hide]


async def main() -> None:
    # Define a geometry consisting of a single AUTD3 device.
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

    # Open the client over an echocat link.
    async with await Client.open(
        geometry,
        echocat.EchocatLinkOption(),
        ClientConfig(),
    ) as client:
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
            modulation.SineOption(sampling_config=SamplingConfig.FREQ_4K),
            mod_buf,
        )

        builder = client.datagram_builder()
        builder.push(SetSilencer())
        builder.push(Pattern(patterns))
        builder.push(Modulation(SamplingConfig.FREQ_4K, mod_buf))
        for frame in builder.build():
            await client.send_checked(frame)

        await asyncio.Event().wait()


if __name__ == "__main__":
    asyncio.run(main())
