import asyncio

import autd3_modulation as modulation
import autd3_pattern as pattern
from autd3 import Client, ClientConfig, Frames, UdpEmulator
from autd3.commands import Modulation, Pattern, SetSilencer, each
from autd3.geometry import Autd3, Geometry, offset
from autd3.units import Hz, m, mm, s
from autd3.value import Intensity, SamplingConfig

async def main() -> None:
    geometry = Geometry([Autd3(), Autd3()])
    emulator = UdpEmulator(geometry.num_devices())
    async with await Client.open(geometry, emulator.option(), ClientConfig()) as client:
        wavelength = pattern.wavelength(340 * m / s)
        left = geometry.phase_buffer()
        pattern.focus(geometry, geometry.center() + offset(-40.0 * mm, 0.0 * mm, 150.0 * mm), wavelength, left)
        right = geometry.phase_buffer()
        pattern.focus(geometry, geometry.center() + offset(40.0 * mm, 0.0 * mm, 150.0 * mm), wavelength, right)

        data = modulation.modulation_buffer()
        modulation.sine(150 * Hz, modulation.SineOption(), data)

        # ANCHOR: api
        await client.send(SetSilencer())

        done = await client.send_streaming(Modulation(SamplingConfig.FREQ_4K, data))
        await done

        frames = Frames.encode(geometry, Pattern(left, Intensity.MAX))
        for frame in frames:
            (await (await client.send_frame(frame))).check()
        # ANCHOR_END: api

        # ANCHOR: each
        await client.send(each(lambda device: Pattern(left if device.idx() % 2 == 0 else right, Intensity.MAX)))
        # ANCHOR_END: each


asyncio.run(main())
