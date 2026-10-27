import asyncio

import autd3_modulation as modulation
import autd3_pattern as pattern
from autd3 import Client, TransportOption
from autd3.commands import Clear, Modulation, Pattern, SetSilencer, Synchronize
from autd3.geometry import Autd3, Geometry, offset
from autd3.units import Hz, m, mm, s
from autd3.value import Intensity, SamplingConfig

# xtask:long-running  # [hide]


async def main() -> None:
    # Define a geometry consisting of a single AUTD3 device.
    geometry = Geometry([Autd3()])

    # Open the client.
    with Client.open(
        geometry,
        TransportOption(),
    ) as client:
        await client.send(Clear())
        await client.send(Synchronize())

        # Generate a focus 150 mm above the array center.
        target = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm)
        wavelength = pattern.wavelength(340 * m / s)
        phases = geometry.phase_buffer()
        pattern.focus(
            geometry,
            target,
            wavelength,
            phases,
        )

        # Apply a 200 Hz sine-wave AM.
        sampling_config = SamplingConfig.FREQ_4K
        mod_buf = modulation.modulation_buffer()
        modulation.sine(
            200 * Hz,
            modulation.SineOption(),
            sampling_config,
            mod_buf,
        )

        await client.send(SetSilencer())
        await client.send(Pattern(phases, Intensity.MAX))
        await client.send(Modulation(sampling_config, mod_buf))

        try:
            await asyncio.Event().wait()
        finally:
            await client.send(Pattern(geometry.phase_buffer(), Intensity.MIN))


if __name__ == "__main__":
    asyncio.run(main())
