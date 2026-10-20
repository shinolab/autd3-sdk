import asyncio

import autd3_modulation as modulation
import autd3_pattern as pattern
from autd3 import Client, TransportOption
from autd3.commands import Clear, Modulation, Pattern, SetSilencer, Synchronize
from autd3.geometry import Autd3, Geometry, offset
from autd3.units import Hz, m, mm, s
from autd3.value import Intensity, SamplingConfig

async def main() -> None:
    geometry = Geometry([Autd3()])

    async with Client.open(
        geometry,
        TransportOption(),
    ) as client:
        await client.send((Clear(), Synchronize()))

        target = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm)
        wavelength = pattern.wavelength(340 * m / s)

        # ANCHOR: pattern_intensity
        phases = geometry.phase_buffer()
        pattern.focus(geometry, target, wavelength, phases)
        intensity = Intensity(0x80)
        # ANCHOR_END: pattern_intensity

        # ANCHOR: modulation
        sampling_config = SamplingConfig.FREQ_4K
        mod_buf = modulation.modulation_buffer()
        modulation.sine(
            200 * Hz,
            modulation.SineOption(
                amplitude=0xFF,
                offset=0x80,
            ),
            sampling_config,
            mod_buf,
        )
        # ANCHOR_END: modulation

        await client.send(SetSilencer())
        await client.send(Pattern(phases, intensity))
        await client.send(Modulation(sampling_config, mod_buf))


if __name__ == "__main__":
    asyncio.run(main())
