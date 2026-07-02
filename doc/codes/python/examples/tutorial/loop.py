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
    foci = []
    autd3.commands.circle(center, 30.0, 20, [0.0, 0.0, 1.0], autd3.value.Intensity.MAX, foci)

    # ANCHOR: infinite
    # By default the playback loops infinitely; B0 keeps circling the focus.
    builder = client.datagram_builder()
    builder.push(autd3.commands.FociStm(autd3.commands.StmConfig(50.0 * Hz), foci, autd3.commands.FociStmOption()))
    for frame in builder.build():
        await client.send_checked(frame)
    # ANCHOR_END: infinite

    # ANCHOR: finite
    # Play the circular motion only 3 times, then stop.
    # A finite loop (and non-immediate transition) only fires when switching to a
    # different bank, so write to bank B1 instead of the current B0.
    builder = client.datagram_builder()
    builder.push(
        autd3.commands.FociStm(
            autd3.commands.StmConfig(50.0 * Hz),
            foci,
            autd3.commands.FociStmOption(
                loop_behavior=autd3.value.LoopBehavior.Finite(3),
                bank=autd3.value.PatternBank.B1,
                transition_mode=autd3.value.TransitionMode.SyncIdx,
            ),
        )
    )
    for frame in builder.build():
        await client.send_checked(frame)
    # ANCHOR_END: finite

    await client.close()


if __name__ == "__main__":
    asyncio.run(main())
