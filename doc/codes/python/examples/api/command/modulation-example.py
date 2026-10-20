import asyncio

from autd3 import Client, TransportOption
from autd3.commands import Clear, Modulation, Synchronize
from autd3.geometry import Autd3, Geometry
from autd3.units import Hz
from autd3.value import SamplingConfig
from autd3_modulation import SineOption, modulation_buffer, sine


async def main() -> None:
    geometry = Geometry([Autd3()])
    async with await Client.open(geometry, TransportOption()) as client:
        await client.send((Clear(), Synchronize()))

        config = SamplingConfig.FREQ_4K
        data = modulation_buffer()
        sine(150 * Hz, SineOption(), config, data)

        await client.send(Modulation(config, data))


asyncio.run(main())
