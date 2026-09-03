import asyncio

import pytest

import autd3
import autd3_core
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


def test_async_with_closes_the_client() -> None:
    async def run() -> None:
        async with await autd3.Client.open(geometry(), nop.Nop(), autd3.ClientConfig()) as client:
            assert client.num_devices() == 1
        with pytest.raises(autd3_core.Autd3Error):
            await client.read_firmware_version()

    asyncio.run(run())


def test_async_with_closes_the_client_on_exception() -> None:
    class Marker(Exception):
        pass

    async def run() -> None:
        opened = await autd3.Client.open(geometry(), nop.Nop(), autd3.ClientConfig())
        with pytest.raises(Marker):
            async with opened as client:
                raise Marker
        with pytest.raises(autd3_core.Autd3Error):
            await client.read_firmware_version()

    asyncio.run(run())


def test_explicit_close_inside_async_with_is_safe() -> None:
    async def run() -> None:
        async with await autd3.Client.open(geometry(), nop.Nop(), autd3.ClientConfig()) as client:
            await client.close()

    asyncio.run(run())


def test_legacy_client_supports_async_with() -> None:
    async def run() -> None:
        async with await autd3.LegacyClient.open(
            geometry(), nop.Nop(), autd3.LegacyClientConfig()
        ) as client:
            assert client.num_devices() == 1
        with pytest.raises(autd3_core.Autd3Error):
            await client.read_firmware_version()

    asyncio.run(run())
