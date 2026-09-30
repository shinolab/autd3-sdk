"""
Multiple AUTD3 devices arranged side by side.

Run with: cargo xtask py example multi_device
"""

import asyncio
import threading

import autd3
from scipy.spatial.transform import Rotation


async def main() -> None:
    geometry = autd3.geometry.Geometry(
        [
            autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0]),
            autd3.geometry.Autd3(
                origin=(autd3.geometry.Autd3.DEVICE_WIDTH, 0.0, 0.0),
                rotation=Rotation.identity(),
            ),
        ]
    )

    driver, connector = autd3.Driver.open(autd3.TransportOption(), geometry.num_devices())
    threading.Thread(target=driver.run, daemon=True).start()
    async with await autd3.Client.open(
        geometry,
        connector,
        autd3.ClientConfig(),
    ) as client:
        print("devices:", client.num_devices())
        for i, fw in enumerate(await client.read_firmware_version()):
            print(f"device[{i}] firmware version: {fw}")

        center = geometry.center()
        print(f"array center: ({center[0]:.2f}, {center[1]:.2f}, {center[2]:.2f}) mm")


if __name__ == "__main__":
    asyncio.run(main())
