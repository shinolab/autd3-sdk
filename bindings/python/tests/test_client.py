import asyncio

import autd3
import autd3_link_nop as nop


def geometry() -> autd3.geometry.Geometry:
    return autd3.geometry.Geometry([autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])


def test_geometry_is_reachable_through_the_client() -> None:
    async def run() -> None:
        geo = geometry()
        client = await autd3.Client.open(geo, nop.Nop(), autd3.ClientConfig())
        assert client.geometry().num_devices() == client.num_devices()
        assert client.geometry().num_transducers() == geo.num_transducers()
        assert len(client.geometry().pattern_buffer()) == geo.num_devices()
        await client.close()

    asyncio.run(run())
