import asyncio
import collections
import math

import autd3_pattern as pattern
from autd3 import Client, Frames, TransportOption
from autd3.commands import Clear, Pattern, SetSilencer, Synchronize
from autd3.geometry import Autd3, Geometry, offset
from autd3.params import MAX_INFLIGHT
from autd3.units import m, mm, s
from autd3.value import Intensity

NUM_POINTS = 1000
RADIUS_MM = 30.0


async def main() -> None:
    geometry = Geometry([Autd3()])

    with Client.open(
        geometry,
        TransportOption(),
    ) as client:
        await client.send(Clear())
        await client.send(Synchronize())

        await client.send(SetSilencer())

        wavelength = pattern.wavelength(340 * m / s)

        # ANCHOR: targets
        # Prepare 1000 focus points along a circle 150 mm above the array center.
        center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm)
        targets = [
            center
            + offset(
                RADIUS_MM * math.cos(2.0 * math.pi * i / NUM_POINTS) * mm,
                RADIUS_MM * math.sin(2.0 * math.pi * i / NUM_POINTS) * mm,
                0.0 * mm,
            )
            for i in range(NUM_POINTS)
        ]
        # ANCHOR_END: targets

        await stop_and_wait(client, geometry, targets, wavelength)
        await streaming(client, geometry, targets, wavelength)

        await client.send(Pattern(geometry.phase_buffer(), Intensity.MIN))
        client.close()


async def stop_and_wait(client, geometry, targets, wavelength) -> None:
    # ANCHOR: stop_and_wait
    phases = geometry.phase_buffer()
    for target in targets:
        pattern.focus(
            geometry,
            target,
            wavelength,
            phases,
        )
        await client.send(Pattern(phases, Intensity.MAX))
    # ANCHOR_END: stop_and_wait


async def streaming(client, geometry, targets, wavelength) -> None:
    # ANCHOR: streaming
    phases = geometry.phase_buffer()
    frames = Frames()
    pending = collections.deque()
    for target in targets:
        pattern.focus(
            geometry,
            target,
            wavelength,
            phases,
        )
        frames.encode_into(geometry, Pattern(phases, Intensity.MAX))
        for frame in frames:
            if len(pending) >= MAX_INFLIGHT:
                (await pending.popleft()).check()
            pending.append(await client.send_frame(frame))
    # Drain the remaining responses.
    while pending:
        (await pending.popleft()).check()
    # ANCHOR_END: streaming


if __name__ == "__main__":
    asyncio.run(main())
