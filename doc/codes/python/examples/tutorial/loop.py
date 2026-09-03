import asyncio

import numpy as np

import autd3_link_echocat as echocat
from autd3 import Client, ClientConfig
from autd3.commands import FociStm, FociStmOption, circle
from autd3.geometry import Autd3, Geometry
from autd3.units import Hz
from autd3.value import Intensity, LoopBehavior, PatternBank, TransitionMode


async def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

    async with await Client.open(
        geometry,
        echocat.EchocatLinkOption(),
        ClientConfig(),
    ) as client:
        center = geometry.center() + np.array([0.0, 0.0, 150.0])
        foci = []
        circle(center, 30.0, 20, [0.0, 0.0, 1.0], Intensity.MAX, foci)

        # ANCHOR: infinite
        # By default the playback loops infinitely; B0 keeps circling the focus.
        builder = client.datagram_builder()
        builder.push(FociStm(50.0 * Hz, foci, FociStmOption()))
        for frame in builder.build():
            await client.send_checked(frame)
        # ANCHOR_END: infinite

        # ANCHOR: finite
        # Play the circular motion only 3 times, then stop.
        # A finite loop (and non-immediate transition) only fires when switching to a
        # different bank, so write to bank B1 instead of the current B0.
        builder = client.datagram_builder()
        builder.push(
            FociStm(
                50.0 * Hz,
                foci,
                FociStmOption(
                    loop_behavior=LoopBehavior.Finite(3),
                    bank=PatternBank.B1,
                    transition_mode=TransitionMode.SyncIdx,
                ),
            )
        )
        for frame in builder.build():
            await client.send_checked(frame)
        # ANCHOR_END: finite


if __name__ == "__main__":
    asyncio.run(main())
