import asyncio

from autd3 import Client, ClientConfig, UdpOption
from autd3.commands import ActivatePatternBank, ConfigPattern, WritePatternBuffer
from autd3.geometry import Autd3, Geometry, offset
from autd3.units import m, mm, s
from autd3.value import Intensity, LoopBehavior, PatternBank, SamplingConfig, TransitionMode
from autd3_pattern import focus, wavelength


async def main() -> None:
    geometry = Geometry([Autd3()])
    async with await Client.open(geometry, UdpOption(), ClientConfig()) as client:
        phases = geometry.phase_buffer()
        focus(
            geometry,
            geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm),
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
