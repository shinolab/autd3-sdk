import asyncio
import math

import numpy as np

from autd3 import Client, TransportOption
from autd3.commands import Clear, FociStm, FociStmOption, Pattern, Synchronize
from autd3.geometry import Autd3, Geometry, offset
from autd3.units import Hz, mm
from autd3.value import ControlPoint, ControlPoints, Intensity, LoopBehavior, PatternBank, Phase, SamplingConfig, TransitionMode


async def main() -> None:
    geometry = Geometry([Autd3()])

    with Client.open(
        geometry,
        TransportOption(),
    ) as client:
        await client.send(Clear())
        await client.send(Synchronize())

        center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm)
        radius = (30.0 * mm).mm
        foci = []
        for i in range(20):
            theta = 2.0 * math.pi * i / 20.0
            p = center + np.array([radius * math.cos(theta), radius * math.sin(theta), 0.0])
            foci.append(ControlPoints([ControlPoint(p, Phase.ZERO)], Intensity.MAX))
        config = SamplingConfig(50.0 * Hz * len(foci))

        # ANCHOR: infinite
        # By default the playback loops infinitely; B0 keeps circling the focus.
        await client.send(FociStm(config, foci, FociStmOption()))
        # ANCHOR_END: infinite

        # ANCHOR: finite
        # Play the circular motion only 3 times, then stop.
        # A finite loop (and non-immediate transition) only fires when switching to a
        # different bank, so write to bank B1 instead of the current B0.
        await client.send(
            FociStm(
                config,
                foci,
                FociStmOption(
                    loop_behavior=LoopBehavior.Finite(3),
                    bank=PatternBank.B1,
                    transition_mode=TransitionMode.SyncIdx,
                ),
            )
        )
        # ANCHOR_END: finite

        await client.send(Pattern(geometry.phase_buffer(), Intensity.MIN))
        client.close()


if __name__ == "__main__":
    asyncio.run(main())
