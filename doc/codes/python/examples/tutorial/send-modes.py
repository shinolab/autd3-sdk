import asyncio
import collections
import math

import numpy as np

import autd3_link_echocat as echocat
import autd3_pattern as pattern
from autd3 import MAX_INFLIGHT, Client, ClientConfig
from autd3.commands import Pattern, SetSilencer
from autd3.geometry import Autd3, Geometry
from autd3.units import m, s

NUM_POINTS = 1000
RADIUS_MM = 30.0


async def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

    async with await Client.open(
        geometry,
        echocat.EchocatLinkOption(),
        ClientConfig(),
    ) as client:
        builder = client.datagram_builder()
        builder.push(SetSilencer())
        for frame in builder.build():
            await client.send_checked(frame)

        wavelength = pattern.wavelength(340 * m / s)

        # ANCHOR: targets
        # Prepare 1000 focus points along a circle 150 mm above the array center.
        center = geometry.center() + np.array([0.0, 0.0, 150.0])
        targets = [
            center
            + np.array(
                [
                    RADIUS_MM * math.cos(2.0 * math.pi * i / NUM_POINTS),
                    RADIUS_MM * math.sin(2.0 * math.pi * i / NUM_POINTS),
                    0.0,
                ]
            )
            for i in range(NUM_POINTS)
        ]
        # ANCHOR_END: targets

        await stop_and_wait(client, geometry, targets, wavelength)
        await streaming(client, geometry, targets, wavelength)


async def stop_and_wait(client, geometry, targets, wavelength) -> None:
    # ANCHOR: stop_and_wait
    patterns = geometry.pattern_buffer()
    for target in targets:
        pattern.focus(
            geometry,
            target,
            wavelength,
            pattern.FocusOption(),
            patterns,
        )
        builder = client.datagram_builder()
        builder.push(Pattern(patterns))
        for frame in builder.build():
            await client.send_checked(frame)
    # ANCHOR_END: stop_and_wait


async def streaming(client, geometry, targets, wavelength) -> None:
    # ANCHOR: streaming
    patterns = geometry.pattern_buffer()
    pending = collections.deque()
    for target in targets:
        pattern.focus(
            geometry,
            target,
            wavelength,
            pattern.FocusOption(),
            patterns,
        )
        builder = client.datagram_builder()
        builder.push(Pattern(patterns))
        for frame in builder.build():
            if len(pending) >= MAX_INFLIGHT:
                (await pending.popleft()).check()
            pending.append(await client.send(frame))
    # Drain the remaining responses.
    while pending:
        (await pending.popleft()).check()
    # ANCHOR_END: streaming


if __name__ == "__main__":
    asyncio.run(main())
