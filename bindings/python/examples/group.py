"""
Per-device-group command: focus each device group at a different target.

Run with: cargo xtask py example group
"""

import asyncio
import signal

import autd3
import autd3_pattern as pattern
from autd3.commands import Pattern, SetSilencer, each
from autd3.geometry import Autd3, Geometry, offset
from autd3.units import m, mm, s
from autd3.value import Intensity


async def main() -> None:
    _log_guard = autd3.init_tracing()

    geometry = Geometry(
        [
            Autd3([0.0, 0.0, 0.0]),
            Autd3([Autd3.DEVICE_WIDTH, 0.0, 0.0]),
        ]
    )

    async with await autd3.Client.open(
        geometry,
        autd3.TransportOption(),
        autd3.ClientConfig(),
    ) as client:
        print("devices:", client.num_devices())

        wavelength = pattern.wavelength(340 * m / s)

        left_target = geometry.center() + offset(-40.0 * mm, 0.0 * mm, 150.0 * mm)
        left = geometry.phase_buffer()
        pattern.focus(geometry, left_target, wavelength, left)

        right_target = geometry.center() + offset(40.0 * mm, 0.0 * mm, 150.0 * mm)
        right = geometry.phase_buffer()
        pattern.focus(geometry, right_target, wavelength, right)

        await client.send(SetSilencer())
        await client.send(each(lambda device: Pattern(left if device.idx() % 2 == 0 else right, Intensity.MAX)))

        print("even devices -> left target, odd devices -> right target — press Ctrl+C to stop")
        stop = asyncio.Event()
        loop = asyncio.get_running_loop()
        for sig in (signal.SIGINT, signal.SIGTERM):
            loop.add_signal_handler(sig, stop.set)
        await stop.wait()

        await client.silent_stop()


if __name__ == "__main__":
    asyncio.run(main())
