import asyncio

import numpy as np

import autd3_link_ethercrab as ethercrab
from autd3 import Client, ClientConfig
from autd3.commands import FociStm, FociStmOption, StmConfig
from autd3.geometry import Autd3, Geometry
from autd3.units import Hz
from autd3.value import ControlPoint, ControlPoints

async def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

    client = await Client.open(
        geometry,
        ethercrab.EtherCrabLinkOption(),
        ClientConfig(),
    )

    center = geometry.center() + np.array([0.0, 0.0, 150.0])

    # ANCHOR: stm
    points = [
        ControlPoints([ControlPoint(center + np.array([20.0, 0.0, 0.0]))]),
        ControlPoints([ControlPoint(center + np.array([-20.0, 0.0, 0.0]))]),
    ]
    builder = client.datagram_builder()
    builder.push(FociStm(StmConfig(0.5 * Hz), points, FociStmOption()))
    for frame in builder.build():
        await client.send_checked(frame)
    # ANCHOR_END: stm

    await client.close()


if __name__ == "__main__":
    asyncio.run(main())
