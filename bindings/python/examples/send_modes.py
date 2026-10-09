"""
Sweeps a focus around a circle two ways: stop-and-wait vs streaming.

Run with: cargo xtask py example send_modes
"""

import asyncio
import collections
import math
import time
from typing import Any

import numpy as np
import numpy.typing as npt

import autd3
import autd3_pattern as pattern
from autd3.commands import ConfigPattern, WritePatternBuffer
from autd3.geometry import Autd3, Geometry, offset
from autd3.units import Length, m, mm, s
from autd3.value import Intensity, LoopBehavior, PatternBank, SamplingConfig

TOTAL_POINTS = 1000


async def main() -> None:
    _log_guard = autd3.init_tracing()

    geometry = Geometry([Autd3([0.0, 0.0, 0.0])])

    async with await autd3.Client.open(
        geometry,
        autd3.TransportOption(),
        autd3.ClientConfig(),
    ) as client:
        await configure(client)

        center = geometry.center()
        radius = 30.0
        wavelength = pattern.wavelength(340 * m / s)
        targets = []
        for i in range(TOTAL_POINTS):
            theta = 2.0 * math.pi * i / TOTAL_POINTS
            targets.append(
                center + offset(radius * math.cos(theta) * mm, radius * math.sin(theta) * mm, 150.0 * mm)
            )

        print(f"sweeping a focus through {TOTAL_POINTS} positions, twice")

        elapsed = await run_stop_and_wait(client, targets, wavelength)
        report("stop-and-wait", elapsed)

        elapsed = await run_streaming(client, targets, wavelength, autd3.MAX_INFLIGHT)
        report("streaming", elapsed)

        await client.silent_stop()


async def run_stop_and_wait(
    client: autd3.Client,
    targets: list[npt.NDArray[np.floating[Any]]],
    wavelength: Length,
) -> float:
    geometry = client.geometry()
    phases = geometry.phase_buffer()

    # stop-and-wait: confirm each frame lands before issuing the next.
    start = time.perf_counter()
    for target in targets:
        pattern.focus(geometry, target, wavelength, phases)
        await client.send(write_focus(phases))
    return time.perf_counter() - start


async def run_streaming(
    client: autd3.Client,
    targets: list[npt.NDArray[np.floating[Any]]],
    wavelength: Length,
    max_inflight: int,
) -> float:
    geometry = client.geometry()
    phases = geometry.phase_buffer()
    frames = autd3.Frames()
    pending: collections.deque[autd3.ResponseFuture] = collections.deque()

    # streaming: keep MAX_INFLIGHT frames on the wire, draining the oldest response
    # once the window is full.
    start = time.perf_counter()
    for target in targets:
        pattern.focus(geometry, target, wavelength, phases)
        frames.encode_into(geometry, write_focus(phases))
        for frame in frames:
            if len(pending) >= max_inflight:
                (await pending.popleft()).check()
            pending.append(await client.send_frame(frame))
    while pending:
        (await pending.popleft()).check()
    return time.perf_counter() - start


async def configure(client: autd3.Client) -> None:
    phases = client.geometry().phase_buffer()
    await client.send(
        (
            WritePatternBuffer(PatternBank.B0, 0, phases, Intensity.MIN),
            ConfigPattern(PatternBank.B0, SamplingConfig.FREQ_4K, 1, loop_behavior=LoopBehavior.Infinite),
        )
    )


def write_focus(phases: pattern.PhaseBuffer) -> WritePatternBuffer:
    return WritePatternBuffer(PatternBank.B0, 0, phases, Intensity.MAX)


def report(label: str, elapsed: float) -> None:
    rate = TOTAL_POINTS / elapsed
    print(f"{label}: {TOTAL_POINTS} updates in {elapsed:.2f}s ({rate:.0f} updates/s)")


if __name__ == "__main__":
    asyncio.run(main())
