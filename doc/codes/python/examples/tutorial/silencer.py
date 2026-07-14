import asyncio

import numpy as np

import autd3_link_ethercrab as ethercrab
from autd3 import Client, ClientConfig
from autd3.commands import FixedCompletionTime, FociStm, FociStmOption, SetSilencer
from autd3.geometry import Autd3, Geometry
from autd3.units import Hz
from autd3.value import ControlPoint, ControlPoints, Intensity
from autd3_core import Duration

# xtask:expect-error


async def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

    client = await Client.open(
        geometry,
        ethercrab.EtherCrabLinkOption(),
        ClientConfig(),
    )

    center = geometry.center() + np.array([0.0, 0.0, 150.0])
    radius = 30.0

    # ANCHOR: disable
    foci = []
    for i in range(20):
        theta = 2.0 * np.pi * i / 20.0
        p = center + np.array([radius * np.cos(theta), radius * np.sin(theta), 0.0])
        foci.append(ControlPoints([ControlPoint(p)], Intensity.MAX))
    builder = client.datagram_builder()
    builder.push(SetSilencer.disable())
    builder.push(
        FociStm(
            50.0 * Hz,
            foci,
            FociStmOption(),
        )
    )
    for frame in builder.build():
        await client.send_checked(frame)
    # ANCHOR_END: disable

    # ANCHOR: err
    foci = []
    for i in range(40):
        theta = 2.0 * np.pi * i / 40.0
        p = center + np.array([radius * np.cos(theta), radius * np.sin(theta), 0.0])
        foci.append(ControlPoints([ControlPoint(p)], Intensity.MAX))
    builder = client.datagram_builder()
    builder.push(SetSilencer())
    builder.push(
        FociStm(
            50.0 * Hz,
            foci,
            FociStmOption(),
        )
    )
    for frame in builder.build():
        await client.send_checked(frame)
    # ANCHOR_END: err

    # ANCHOR: workaround
    foci = []
    for i in range(40):
        theta = 2.0 * np.pi * i / 40.0
        p = center + np.array([radius * np.cos(theta), radius * np.sin(theta), 0.0])
        foci.append(ControlPoints([ControlPoint(p)], Intensity.MAX))
    builder = client.datagram_builder()
    builder.push(
        SetSilencer(
            FixedCompletionTime(
                intensity=Duration.from_micros(500),
                phase=Duration.from_micros(500),
                strict_mode=True,
            )
        )
    )
    builder.push(FociStm(50.0 * Hz, foci, FociStmOption()))
    for frame in builder.build():
        await client.send_checked(frame)
    # ANCHOR_END: workaround

    await client.close()


if __name__ == "__main__":
    asyncio.run(main())
