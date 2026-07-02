import asyncio

from autd3 import Client, ClientConfig
from autd3.commands import Modulation
from autd3.geometry import Autd3, Geometry
from autd3.units import Hz
from autd3.value import SamplingConfig
from autd3_link_nop import Nop
from autd3_modulation import SineOption, modulation_buffer, sine


async def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])
    client = await Client.open(geometry, Nop(), ClientConfig())

    data = modulation_buffer()
    sine(150.0 * Hz, SineOption(), data)

    builder = client.datagram_builder()
    builder.push(Modulation(SamplingConfig.FREQ_4K, data))
    frames = builder.build()
    for frame in frames:
        await client.send_checked(frame)

    await client.close()


asyncio.run(main())
