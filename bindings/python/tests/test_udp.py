import asyncio

import pytest

import autd3
import autd3_core


def geometry(n: int) -> autd3.geometry.Geometry:
    return autd3.geometry.Geometry(
        [autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0]) for _ in range(n)]
    )


def test_defaults_follow_the_spec() -> None:
    option = autd3.TransportOption()
    assert option.iface is None
    assert option.group is None
    assert option.cycle.as_millis() == 1
    assert option.reply_timeout.as_millis() == 1
    assert option.response_timeout.as_millis() == 200
    assert option.enumeration_timeout.as_millis() == 10_000
    assert option.sync_timeout.as_millis() == 5_000


def test_fields_round_trip() -> None:
    option = autd3.TransportOption(
        iface="eth1",
        group="[::1]:44336",
        cycle=autd3.Duration.from_millis(2),
        sync_timeout=autd3.Duration.from_secs(3),
    )
    assert option.iface == "eth1"
    assert option.group == "[::1]:44336"
    assert option.cycle.as_millis() == 2
    assert option.sync_timeout.as_millis() == 3_000


def test_a_non_ipv6_group_is_rejected() -> None:
    with pytest.raises(ValueError):
        autd3.TransportOption(group="127.0.0.1:1")


def test_the_client_opens_the_emulated_chain() -> None:
    emulator = autd3.UdpEmulator(2)
    assert emulator.num_devices == 2

    async def run() -> None:
        client, checker = await autd3.Client.open_with_checker(
            geometry(2), emulator.option(), autd3.ClientConfig()
        )
        async with client:
            assert client.num_devices() == 2
            assert len(await client.read_firmware_version()) == 2
            assert checker.check().all_op

    asyncio.run(run())


def test_a_device_count_mismatch_fails_to_open() -> None:
    emulator = autd3.UdpEmulator(1)

    async def run() -> None:
        await autd3.Client.open(geometry(2), emulator.option(), autd3.ClientConfig())

    with pytest.raises(autd3_core.Autd3Error):
        asyncio.run(run())


def test_reboot_rejects_an_out_of_range_index() -> None:
    emulator = autd3.UdpEmulator(1)
    with pytest.raises(IndexError):
        emulator.reboot(1)
