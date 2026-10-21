import asyncio

from autd3 import Client, TransportOption
from autd3.commands import ActivatePatternBank, Clear, ConfigPattern, PhaseDepth, Synchronize, WritePatternPhase
from autd3.geometry import Autd3, Geometry, offset
from autd3.units import Hz, m, mm, s
from autd3.value import Intensity, LoopBehavior, PatternBank, SamplingConfig, TransitionMode
from autd3_pattern import focus
from autd3_pattern import wavelength as calc_wavelength


async def main() -> None:
    geometry = Geometry([Autd3()])
    async with Client.open(geometry, TransportOption()) as client:
        await client.send((Clear(), Synchronize()))

        wavelength = calc_wavelength(340 * m / s)
        patterns = []
        for x in (-30.0, -10.0, 10.0, 30.0):
            buffer = geometry.phase_buffer()
            focus(
                geometry,
                geometry.center() + offset(x * mm, 0.0 * mm, 150.0 * mm),
                wavelength,
                buffer,
            )
            patterns.append(buffer)

        bank = PatternBank.B0

        await client.send(
            WritePatternPhase(
                bank=bank,
                start_index=0,
                depth=PhaseDepth.Bits4,
                intensity=Intensity.MAX,
                patterns=patterns,
            )
        )
        await client.send(
            ConfigPattern(
                bank=bank,
                config=SamplingConfig(1.0 * Hz * len(patterns)),
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
