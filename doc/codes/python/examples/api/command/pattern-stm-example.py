import asyncio
import math

import numpy as np

from autd3 import Client, ClientConfig, UdpEmulator
from autd3.commands import PatternStm, PatternStmOption, PhaseDepth
from autd3.geometry import Autd3, Geometry
from autd3.units import Hz, m, s
from autd3.value import Intensity, LoopBehavior, PatternBank, TransitionMode
from autd3_pattern import focus
from autd3_pattern import wavelength as calc_wavelength


async def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])
    emulator = UdpEmulator(geometry.num_devices())
    async with await Client.open(geometry, emulator.option(), ClientConfig()) as client:
        center = geometry.center() + np.array([0.0, 0.0, 150.0])
        wavelength = calc_wavelength(340 * m / s)
        patterns = []
        for i in range(200):
            theta = 2.0 * math.pi * i / 200
            target = center + np.array([30.0 * math.cos(theta), 30.0 * math.sin(theta), 0.0])
            buffer = geometry.phase_buffer()
            focus(
                geometry,
                target,
                wavelength,
                buffer,
            )
            patterns.append(buffer)

        await client.send(
            PatternStm(
                1.0 * Hz,
                patterns,
                Intensity.MAX,
                PatternStmOption(
                    bank=PatternBank.B0,
                    phase_depth=PhaseDepth.Bits8,
                    loop_behavior=LoopBehavior.Infinite,
                    transition_mode=TransitionMode.Immediate,
                ),
            )
        )


asyncio.run(main())
