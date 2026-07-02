"""Multiple AUTD3 devices arranged side by side. Run with: cargo xtask py example multi_device"""

import asyncio

import autd3
import autd3_link_ethercrab as ethercrab


async def main() -> None:
    geometry = autd3.geometry.Geometry(
        [
            autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0]),
            autd3.geometry.Autd3(
                origin=(autd3.geometry.Autd3.DEVICE_WIDTH, 0.0, 0.0),
                rotation=(1.0, 0.0, 0.0, 0.0),
            ),
        ]
    )

    client = await autd3.Client.open(
        geometry,
        ethercrab.EtherCrabLinkOption(),
        autd3.ClientConfig(),
    )

    print("devices:", client.num_devices())
    for i, fw in enumerate(await client.read_firmware_version()):
        print(f"device[{i}] firmware version: {fw}")

    center = geometry.center()
    print(f"array center: ({center[0]:.2f}, {center[1]:.2f}, {center[2]:.2f}) mm")

    await client.close()


if __name__ == "__main__":
    asyncio.run(main())
