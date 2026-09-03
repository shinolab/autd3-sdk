"""
Per-device-group command: focus each device group at a different target.

Run with: cargo xtask py example group
"""

import asyncio
import signal

import numpy as np

import autd3
import autd3_link_echocat as echocat
import autd3_pattern as pattern
from autd3.geometry import Autd3
from autd3.units import m, s
from scipy.spatial.transform import Rotation


async def main() -> None:
    geometry = autd3.geometry.Geometry(
        [
            autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0]),
            autd3.geometry.Autd3(origin=(Autd3.DEVICE_WIDTH, 0.0, 0.0), rotation=Rotation.identity()),
        ]
    )

    async with await autd3.Client.open(
        geometry,
        echocat.EchocatLinkOption(),
        autd3.ClientConfig(),
    ) as client:
        print("devices:", client.num_devices())

        wavelength = pattern.wavelength(340 * m / s)
        focus_option = pattern.FocusOption()

        left_target = geometry.center() + np.array([-40.0, 0.0, 150.0])
        left = geometry.pattern_buffer()
        pattern.focus(geometry, left_target, wavelength, focus_option, left)

        right_target = geometry.center() + np.array([40.0, 0.0, 150.0])
        right = geometry.pattern_buffer()
        pattern.focus(geometry, right_target, wavelength, focus_option, right)

        builder = client.datagram_builder()
        builder.push(autd3.commands.SetSilencer())
        builder.push_each(lambda device: autd3.commands.Pattern(left if device.idx() % 2 == 0 else right))
        for frame in builder.build():
            await client.send_checked(frame)

        print("even devices -> left target, odd devices -> right target — press Ctrl+C to stop")
        stop = asyncio.Event()
        loop = asyncio.get_running_loop()
        for sig in (signal.SIGINT, signal.SIGTERM):
            loop.add_signal_handler(sig, stop.set)
        await stop.wait()


if __name__ == "__main__":
    asyncio.run(main())
