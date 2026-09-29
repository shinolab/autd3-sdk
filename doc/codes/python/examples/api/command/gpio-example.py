import asyncio

from autd3 import Client, ClientConfig, UdpEmulator
from autd3.commands import GpioOut, SetGpioOut
from autd3.geometry import Autd3, Geometry


async def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])
    emulator = UdpEmulator(geometry.num_devices())
    async with await Client.open(geometry, emulator.option(), ClientConfig()) as client:
        builder = client.datagram_builder()
        builder.push(
            SetGpioOut(
                outputs=[
                    GpioOut.PatternBank,
                    GpioOut.Thermo,
                    GpioOut.PwmOut(0),
                    GpioOut.Off,
                ]
            )
        )
        frames = builder.build()
        for frame in frames:
            await client.send_checked(frame)


asyncio.run(main())
