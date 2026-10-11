import asyncio

import numpy as np

import autd3_modulation as modulation
import autd3_pattern as pattern
from autd3 import Client, ClientConfig, TransportOption
from autd3.commands import Modulation, Pattern, SetSilencer
from autd3.geometry import Autd3, Geometry
from autd3.units import Hz, m, s
from autd3.value import Intensity, SamplingConfig

async def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

    async with await Client.open(
        geometry,
        TransportOption(),
        ClientConfig(),
    ) as client:
        target = geometry.center() + np.array([0.0, 0.0, 150.0])
        wavelength = pattern.wavelength(340 * m / s)

        # ANCHOR: pattern_intensity
        phases = geometry.phase_buffer()
        pattern.focus(geometry, target, wavelength, phases)
        intensity = Intensity(0x80)
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

        await client.send(SetSilencer())
        await client.send(Pattern(phases, intensity))
        await client.send(Modulation(SamplingConfig.FREQ_4K, mod_buf))


if __name__ == "__main__":
    asyncio.run(main())
