import asyncio
import collections
import math

import numpy as np

import autd3
import autd3_link_ethercrab as ethercrab
import autd3_pattern as pattern
from autd3.units import m, s

NUM_POINTS = 1000
RADIUS_MM = 30.0


async def main() -> None:
    geometry = autd3.geometry.Geometry([autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

    client = await autd3.Client.open(
        geometry,
        ethercrab.EtherCrabLinkOption(),
        autd3.ClientConfig(),
    )

    builder = client.datagram_builder()
    builder.push(autd3.commands.SetSilencer())
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

    await client.stop()
    await client.close()


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
        builder.push(autd3.commands.Pattern(patterns))
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
        builder.push(autd3.commands.Pattern(patterns))
        for frame in builder.build():
            if len(pending) >= autd3.MAX_IN_FLIGHT:
                await pending.popleft()
            pending.append(await client.send(frame))
    # Drain the remaining responses.
    while pending:
        await pending.popleft()
    # ANCHOR_END: streaming


if __name__ == "__main__":
    asyncio.run(main())
