import asyncio

import numpy as np

import autd3
import autd3_link_ethercrab as ethercrab
from autd3.units import Hz
from autd3_core import Duration

# xtask:expect-error


async def main() -> None:
    geometry = autd3.geometry.Geometry([autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

    client = await autd3.Client.open(
        geometry,
        ethercrab.EtherCrabLinkOption(),
        autd3.ClientConfig(),
    )

    center = geometry.center() + np.array([0.0, 0.0, 150.0])

    # ANCHOR: disable
    foci = []
    autd3.commands.circle(center, 30.0, 20, [0.0, 0.0, 1.0], autd3.value.Intensity.MAX, foci)
    builder = client.datagram_builder()
    builder.push(autd3.commands.SetSilencer.disable())
    builder.push(
        autd3.commands.FociStm(
            autd3.commands.StmConfig(50.0 * Hz),
            foci,
            autd3.commands.FociStmOption(),
        )
    )
    for frame in builder.build():
        await client.send_checked(frame)
    # ANCHOR_END: disable

    # ANCHOR: err
    foci = []
    autd3.commands.circle(center, 30.0, 40, [0.0, 0.0, 1.0], autd3.value.Intensity.MAX, foci)
    builder = client.datagram_builder()
    builder.push(autd3.commands.SetSilencer())
    builder.push(
        autd3.commands.FociStm(
            autd3.commands.StmConfig(50.0 * Hz),
            foci,
            autd3.commands.FociStmOption(),
        )
    )
    for frame in builder.build():
        await client.send_checked(frame)
    # ANCHOR_END: err

    # ANCHOR: workaround
    foci = []
    autd3.commands.circle(center, 30.0, 40, [0.0, 0.0, 1.0], autd3.value.Intensity.MAX, foci)
    builder = client.datagram_builder()
    builder.push(
        autd3.commands.SetSilencer(
            autd3.commands.FixedCompletionTime(
                intensity=Duration.from_micros(500),
                phase=Duration.from_micros(500),
                strict_mode=True,
            )
        )
    )
    builder.push(autd3.commands.FociStm(autd3.commands.StmConfig(50.0 * Hz), foci, autd3.commands.FociStmOption()))
    for frame in builder.build():
        await client.send_checked(frame)
    # ANCHOR_END: workaround

    await client.close()


if __name__ == "__main__":
    asyncio.run(main())
