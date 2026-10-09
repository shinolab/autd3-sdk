"""
Multiple AUTD3 devices arranged side by side.

Run with: cargo xtask py example multi_device
"""

import asyncio

import autd3
from autd3.geometry import Autd3, Geometry


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
        for i, fw in enumerate(await client.read_firmware_version()):
            print(f"device[{i}] firmware version: {fw}")

        center = geometry.center()
        print(f"array center: ({center[0]:.2f}, {center[1]:.2f}, {center[2]:.2f}) mm")


if __name__ == "__main__":
    asyncio.run(main())
