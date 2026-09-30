import asyncio
import threading

import numpy as np

import autd3_modulation as modulation
import autd3_pattern as pattern
from autd3 import Client, ClientConfig, Driver, UdpEmulator
from autd3.commands import Pattern, SetSilencer
from autd3.geometry import Autd3, Geometry
from autd3.units import Hz, m, s
from autd3.value import Intensity

async def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0]), Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])
    emulator = UdpEmulator(geometry.num_devices())
    driver, connector = Driver.open(emulator.option(), geometry.num_devices())
    threading.Thread(target=driver.run, daemon=True).start()
    async with await Client.open(geometry, connector, ClientConfig()) as client:
        # ANCHOR: api
        builder = client.datagram_builder()
        builder.push(SetSilencer())
        frames = builder.build()
        for frame in frames:
            await client.send_checked(frame)
        # ANCHOR_END: api

        wavelength = pattern.wavelength(340 * m / s)
        left = geometry.phase_buffer()
        pattern.focus(geometry, geometry.center() + np.array([-40.0, 0.0, 150.0]), wavelength, left)
        right = geometry.phase_buffer()
        pattern.focus(geometry, geometry.center() + np.array([40.0, 0.0, 150.0]), wavelength, right)

        # ANCHOR: push_each
        builder = client.datagram_builder()
        builder.push_each(lambda device: Pattern(left if device.idx() % 2 == 0 else right, Intensity.MAX))
        frames = builder.build()
        # ANCHOR_END: push_each

        for frame in frames:
            await client.send_checked(frame)


asyncio.run(main())
