import asyncio

from autd3 import Client, ClientConfig, UdpOption
from autd3.commands import GpioOut, SetGpioOut
from autd3.geometry import Autd3, Geometry


async def main() -> None:
    geometry = Geometry([Autd3()])
    async with await Client.open(geometry, UdpOption(), ClientConfig()) as client:
        await client.send(
            SetGpioOut(
                outputs=[
                    GpioOut.PatternBank,
                    GpioOut.Thermo,
                    GpioOut.PwmOut(0),
                    GpioOut.Off,
                ]
            )
        )


asyncio.run(main())
