import asyncio

import numpy as np

from autd3 import Client, TransportOption
from autd3.commands import Clear, FixedCompletionTime, FociStm, FociStmOption, SetSilencer, Synchronize
from autd3.geometry import Autd3, Geometry, offset
from autd3.units import Hz, mm
from autd3.value import ControlPoint, ControlPoints, Intensity, Phase
from autd3_core import Duration

# xtask:expect-error


async def main() -> None:
    geometry = Geometry([Autd3()])

    async with await Client.open(
        geometry,
        TransportOption(),
    ) as client:
        await client.send((Clear(), Synchronize()))

        center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm)
        radius = 30.0

        # ANCHOR: disable
        foci = []
        for i in range(20):
            theta = 2.0 * np.pi * i / 20.0
            p = center + np.array([radius * np.cos(theta), radius * np.sin(theta), 0.0])
            foci.append(ControlPoints([ControlPoint(p, Phase.ZERO)], Intensity.MAX))
        await client.send(SetSilencer.disable())
        await client.send(
            FociStm(
                50.0 * Hz,
                foci,
                FociStmOption(),
            )
        )
        # ANCHOR_END: disable

        # ANCHOR: err
        foci = []
        for i in range(40):
            theta = 2.0 * np.pi * i / 40.0
            p = center + np.array([radius * np.cos(theta), radius * np.sin(theta), 0.0])
            foci.append(ControlPoints([ControlPoint(p, Phase.ZERO)], Intensity.MAX))
        await client.send(SetSilencer())
        await client.send(
            FociStm(
                50.0 * Hz,
                foci,
                FociStmOption(),
            )
        )
        # ANCHOR_END: err

        # ANCHOR: workaround
        foci = []
        for i in range(40):
            theta = 2.0 * np.pi * i / 40.0
            p = center + np.array([radius * np.cos(theta), radius * np.sin(theta), 0.0])
            foci.append(ControlPoints([ControlPoint(p, Phase.ZERO)], Intensity.MAX))
        await client.send(
            SetSilencer(
                FixedCompletionTime(
                    intensity=Duration.from_micros(500),
                    phase=Duration.from_micros(500),
                    strict_mode=True,
                )
            )
        )
        await client.send(FociStm(50.0 * Hz, foci, FociStmOption()))
        # ANCHOR_END: workaround


if __name__ == "__main__":
    asyncio.run(main())
