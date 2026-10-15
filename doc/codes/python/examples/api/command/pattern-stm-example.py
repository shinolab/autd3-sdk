import asyncio
import math

from autd3 import Client, ClientConfig, UdpEmulator
from autd3.commands import PatternStm, PatternStmOption, PhaseDepth
from autd3.geometry import Autd3, Geometry, offset
from autd3.units import Hz, m, mm, s
from autd3.value import Intensity, LoopBehavior, PatternBank, TransitionMode
from autd3_pattern import focus
from autd3_pattern import wavelength as calc_wavelength


async def main() -> None:
    geometry = Geometry([Autd3()])
    emulator = UdpEmulator(geometry.num_devices())
    async with await Client.open(geometry, emulator.option(), ClientConfig()) as client:
        center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm)
        wavelength = calc_wavelength(340 * m / s)
        patterns = []
        for i in range(200):
            theta = 2.0 * math.pi * i / 200
            target = center + offset(30.0 * math.cos(theta) * mm, 30.0 * math.sin(theta) * mm, 0.0 * mm)
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
