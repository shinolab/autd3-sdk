import asyncio

import numpy as np

from autd3 import Client, ClientConfig, UdpEmulator
from autd3.commands import ActivatePatternBank, ConfigPattern, PhaseDepth, StmConfig, WritePatternPhase
from autd3.geometry import Autd3, Geometry
from autd3.units import Hz, m, s
from autd3.value import Intensity, LoopBehavior, PatternBank, TransitionMode
from autd3_pattern import focus
from autd3_pattern import wavelength as calc_wavelength


async def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])
    emulator = UdpEmulator(geometry.num_devices())
    async with await Client.open(geometry, emulator.option(), ClientConfig()) as client:
        wavelength = calc_wavelength(340 * m / s)
        patterns = []
        for x in (-30.0, -10.0, 10.0, 30.0):
            buffer = geometry.phase_buffer()
            focus(
                geometry,
                geometry.center() + np.array([x, 0.0, 150.0]),
                wavelength,
                buffer,
            )
            patterns.append(buffer)

        bank = PatternBank.B0

        await client.send(
            WritePatternPhase(
                bank=bank,
                index=0,
                depth=PhaseDepth.Bits4,
                intensity=Intensity.MAX,
                patterns=patterns,
            )
        )
        await client.send(
            ConfigPattern(
                bank=bank,
                config=StmConfig(1.0 * Hz).into_sampling_config(len(patterns)),
                size=len(patterns),
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
