"""Sweeps a focus around a circle two ways: stop-and-wait vs streaming.

Both languages pipeline the streaming mode with two-stage await: `send` enqueues a
frame and returns a ResponseFuture, and a FIFO deque bounds the in-flight frames.
Run with: cargo xtask py example send_modes"""

import asyncio
import collections
import math
import time

import numpy as np

import autd3
import autd3_link_ethercrab as ethercrab
import autd3_pattern as pattern
from autd3.units import m, s

TOTAL_POINTS = 1000


def report(label: str, elapsed: float) -> None:
    rate = TOTAL_POINTS / elapsed
    print(f"{label}: {TOTAL_POINTS} updates in {elapsed:.2f}s ({rate:.0f} updates/s)")


async def configure(client: autd3.Client, patterns: object) -> None:
    pattern.null(patterns)
    builder = client.datagram_builder()
    builder.push(autd3.commands.WritePatternBuffer(autd3.value.PatternBank.B0, 0, patterns))
    builder.push(autd3.commands.ConfigPattern(autd3.value.PatternBank.B0, autd3.value.SamplingConfig.FREQ_4K, 1))
    for frame in builder.build():
        await client.send_checked(frame)


def write_focus(client: autd3.Client, patterns: object) -> object:
    builder = client.datagram_builder()
    builder.push(autd3.commands.WritePatternBuffer(autd3.value.PatternBank.B0, 0, patterns))
    return builder.build()


async def main() -> None:
    geometry = autd3.geometry.Geometry([autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

    client = await autd3.Client.open(
        geometry,
        ethercrab.EtherCrabLinkOption(),
        autd3.ClientConfig(),
    )

    center = geometry.center()
    radius = 30.0
    wavelength = pattern.wavelength(340 * m / s)

    patterns = geometry.pattern_buffer()
    await configure(client, patterns)

    datagrams = []
    for i in range(TOTAL_POINTS):
        theta = 2.0 * math.pi * i / TOTAL_POINTS
        target = center + np.array([radius * math.cos(theta), radius * math.sin(theta), 150.0])
        pattern.focus(geometry, target, wavelength, pattern.FocusOption(), patterns)
        datagrams.append(write_focus(client, patterns))

    print(f"sweeping a focus through {TOTAL_POINTS} positions, twice")

    # stop-and-wait: confirm each frame lands before issuing the next.
    start = time.perf_counter()
    for dg in datagrams:
        for frame in dg:
            await client.send_checked(frame)
    report("stop-and-wait", time.perf_counter() - start)

    # streaming: keep MAX_IN_FLIGHT frames on the wire, draining the oldest response
    # once the window is full.
    start = time.perf_counter()
    pending = collections.deque()
    for dg in datagrams:
        for frame in dg:
            if len(pending) >= autd3.MAX_IN_FLIGHT:
                await pending.popleft()
            pending.append(await client.send(frame))
    while pending:
        await pending.popleft()
    report("streaming", time.perf_counter() - start)

    await client.stop()
    await client.close()


if __name__ == "__main__":
    asyncio.run(main())
