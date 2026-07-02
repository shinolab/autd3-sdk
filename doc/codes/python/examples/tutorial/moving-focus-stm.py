import asyncio

import numpy as np

import autd3
import autd3_link_ethercrab as ethercrab
from autd3.units import Hz


async def main() -> None:
    geometry = autd3.geometry.Geometry([autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

    client = await autd3.Client.open(
        geometry,
        ethercrab.EtherCrabLinkOption(),
        autd3.ClientConfig(),
    )

    center = geometry.center() + np.array([0.0, 0.0, 150.0])

    # ANCHOR: stm
    points = [
        autd3.value.ControlPoints([autd3.value.ControlPoint(center + np.array([20.0, 0.0, 0.0]))]),
        autd3.value.ControlPoints([autd3.value.ControlPoint(center + np.array([-20.0, 0.0, 0.0]))]),
    ]
    builder = client.datagram_builder()
    builder.push(autd3.commands.FociStm(autd3.commands.StmConfig(0.5 * Hz), points, autd3.commands.FociStmOption()))
    for frame in builder.build():
        await client.send_checked(frame)
    # ANCHOR_END: stm

    await client.close()


if __name__ == "__main__":
    asyncio.run(main())
