import asyncio

import numpy as np

import autd3_link_echocat as echocat
from autd3 import Client, ClientConfig
from autd3.commands import FociStm, FociStmOption
from autd3.geometry import Autd3, Geometry
from autd3.units import Hz
from autd3.value import ControlPoint, ControlPoints

async def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

    async with await Client.open(
        geometry,
        echocat.EchocatLinkOption(),
        ClientConfig(),
    ) as client:
        center = geometry.center() + np.array([0.0, 0.0, 150.0])

        # ANCHOR: stm
        points = [
            ControlPoints([ControlPoint(center + np.array([20.0, 0.0, 0.0]))]),
            ControlPoints([ControlPoint(center + np.array([-20.0, 0.0, 0.0]))]),
        ]
        builder = client.datagram_builder()
        builder.push(FociStm(0.5 * Hz, points, FociStmOption()))
        for frame in builder.build():
            await client.send_checked(frame)
        # ANCHOR_END: stm


if __name__ == "__main__":
    asyncio.run(main())
