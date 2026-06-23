"""Sweeps a focus around a circle two ways: stop-and-wait vs streaming.

Rust pipelines the streaming mode with two-stage await; Python expresses the same
"keep N frames in flight" idea with asyncio concurrency bounded by a semaphore.
Run with: cargo xtask py example send_modes"""

import asyncio
import math
import time

import numpy as np

import autd3
import autd3_link_ethercrab as ethercrab
import autd3_pattern as pattern

TOTAL_POINTS = 1000
MAX_INFLIGHT = 127


def report(label: str, elapsed: float) -> None:
    rate = TOTAL_POINTS / elapsed
    print(f"{label}: {TOTAL_POINTS} updates in {elapsed:.2f}s ({rate:.0f} updates/s)")


async def configure(client: autd3.Client, patterns: object) -> None:
    pattern.null(patterns)
    builder = client.datagram_builder()
    builder.push(autd3.WritePatternBuffer(autd3.PatternBank.B0, 0, patterns))
    builder.push(autd3.ConfigPattern(autd3.PatternBank.B0, 1, 1, autd3.PatternDataType.Raw))
    for frame in builder.build():
        await client.send_checked(frame)


def write_focus(client: autd3.Client, patterns: object) -> object:
    builder = client.datagram_builder()
    builder.push(autd3.WritePatternBuffer(autd3.PatternBank.B0, 0, patterns))
    return builder.build()


async def main() -> None:
    geometry = autd3.Geometry([autd3.Autd3()])

    client = await autd3.Client.open(
        geometry,
        ethercrab.EtherCrabLinkOption(),
        autd3.ClientConfig(),
    )

    center = geometry.center()
    radius = 30.0
    wavelength = pattern.wavelength(340_000.0)

    patterns = client.pattern_buffer()
    await configure(client, patterns)

    datagrams = []
    for i in range(TOTAL_POINTS):
        theta = 2.0 * math.pi * i / TOTAL_POINTS
        target = center + np.array([radius * math.cos(theta), radius * math.sin(theta), 150.0])
        pattern.focus(geometry, target, wavelength, autd3.Intensity.MAX, patterns)
        datagrams.append(write_focus(client, patterns))

    print(f"sweeping a focus through {TOTAL_POINTS} positions, twice")

    # stop-and-wait: confirm each frame lands before issuing the next.
    start = time.perf_counter()
    for dg in datagrams:
        for frame in dg:
            await client.send_checked(frame)
    report("stop-and-wait", time.perf_counter() - start)

    # streaming: keep MAX_INFLIGHT frames on the wire concurrently.
    sem = asyncio.Semaphore(MAX_INFLIGHT)

    async def send(frame: object) -> None:
        async with sem:
            await client.send_checked(frame)

    start = time.perf_counter()
    await asyncio.gather(*(send(frame) for dg in datagrams for frame in dg))
    report("streaming", time.perf_counter() - start)

    await client.stop()
    await client.close()


if __name__ == "__main__":
    asyncio.run(main())
