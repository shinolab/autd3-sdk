import asyncio

import numpy as np

from autd3 import Client, ClientConfig, UdpEmulator
from autd3.commands import ActivatePatternBank, ConfigPattern, WritePatternBuffer
from autd3.geometry import Autd3, Geometry
from autd3.units import m, s
from autd3.value import Intensity, LoopBehavior, PatternBank, SamplingConfig, TransitionMode
from autd3_pattern import focus, wavelength


async def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])
    emulator = UdpEmulator(geometry.num_devices())
    async with await Client.open(geometry, emulator.option(), ClientConfig()) as client:
        phases = geometry.phase_buffer()
        focus(
            geometry,
            geometry.center() + np.array([0.0, 0.0, 150.0]),
            wavelength(340 * m / s),
            phases,
        )

        bank = PatternBank.B0

        await client.send(
            WritePatternBuffer(
                bank=bank,
                index=0,
                phases=phases,
                intensities=Intensity.MAX,
            )
        )
        await client.send(
            ConfigPattern(
                bank=bank,
                config=SamplingConfig(0xFFFF),
                size=1,
                loop_behavior=LoopBehavior.Infinite,
            )
        )
        await client.send(
            ActivatePatternBank(
                bank=bank,
                transition_mode=TransitionMode.Immediate,
            )
        )


asyncio.run(main())
