import asyncio
import sys
from typing import Any

import pytest

import autd3
import autd3_core
import autd3_modulation


def geometry(n: int) -> autd3.geometry.Geometry:
    return autd3.geometry.Geometry(
        [autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0]) for _ in range(n)]
    )


def loose(value: object) -> Any:
    return value


def test_defaults_follow_the_spec() -> None:
    option = autd3.UdpOption()
    assert option.iface == autd3.Interface.Auto
    assert option.heartbeat == autd3.Duration.from_millis(10)
    assert option.reply_timeout == autd3.Duration.from_millis(1)
    assert option.lost_timeout == autd3.Duration.from_millis(100)
    assert option.recovery_interval is None
    assert option.response_timeout == autd3.Duration.from_millis(200)
    assert option.enumeration_timeout == autd3.Duration.from_secs(10)
    assert option.sync_timeout == autd3.Duration.from_secs(30)
    assert option.send_rate_limit is None
    assert option.send_buffer == 8192
    assert option.timer_resolution == autd3.Duration.from_millis(1)


def test_fields_round_trip() -> None:
    option = autd3.UdpOption(
        iface="eth1",
        heartbeat=autd3.Duration.from_millis(2),
        lost_timeout=autd3.Duration.from_millis(50),
        sync_timeout=autd3.Duration.from_secs(3),
        send_rate_limit=95.0,
        recovery_interval=autd3.Duration.from_millis(100),
    )
    assert option.recovery_interval == autd3.Duration.from_millis(100)
    assert option.iface == autd3.Interface.Name("eth1")
    assert option.heartbeat == autd3.Duration.from_millis(2)
    assert autd3.UdpOption(heartbeat=None).heartbeat is None
    assert autd3.UdpOption(iface="eth0").heartbeat == autd3.Duration.from_millis(10)
    assert option.lost_timeout == autd3.Duration.from_millis(50)
    assert option.sync_timeout == autd3.Duration.from_secs(3)
    assert option.send_rate_limit == 95.0
    assert autd3.UdpOption(timer_resolution=None).timer_resolution is None
    assert autd3.UdpOption(send_buffer=None).send_buffer is None
    assert autd3.UdpOption(send_buffer=65536).send_buffer == 65536
    assert autd3.UdpOption(
        timer_resolution=autd3.Duration.from_millis(2)
    ).timer_resolution == autd3.Duration.from_millis(2)


def test_the_interface_selects_where_to_connect() -> None:
    assert autd3.UdpOption(iface=None).iface == autd3.Interface.Auto
    simulator = autd3.UdpOption(iface=autd3.Interface.Simulator)
    assert simulator.iface == autd3.Interface.Simulator
    assert simulator.iface != autd3.Interface.Auto
    assert repr(autd3.Interface.Name("eth0")) == 'Interface.Name("eth0")'
    assert autd3.Interface.Name("eth0").name() == "eth0"
    assert autd3.Interface.Auto.name() is None
    assert autd3.Interface.Simulator.name() is None
    assert hash(autd3.Interface.Name("eth0")) == hash(autd3.Interface.Name("eth0"))
    assert len({autd3.Interface.Auto, autd3.Interface.Simulator, autd3.Interface.Auto}) == 2
    with pytest.raises(TypeError):
        autd3.UdpOption(iface=loose(1))


def test_an_address_interface_round_trips_and_rejects_garbage() -> None:
    iface = autd3.Interface.Addr("[::1]:44336")
    assert repr(iface) == 'Interface.Addr("[::1]:44336")'
    assert autd3.UdpOption(iface=iface).iface == iface
    assert iface != autd3.Interface.Simulator
    with pytest.raises(ValueError):
        autd3.Interface.Addr("127.0.0.1:1")


def test_device_states_are_values() -> None:
    assert autd3.DeviceState.Ready == autd3.DeviceState.Ready
    assert autd3.DeviceState.Ready != autd3.DeviceState.Lost
    assert len({autd3.DeviceState.Ready, autd3.DeviceState.Syncing, autd3.DeviceState.Lost}) == 3
    assert repr(autd3.DeviceState.Syncing) == "DeviceState.Syncing"
    assert str(autd3.DeviceState.Lost) == "LOST"


def test_the_client_config_reads_back_its_fields() -> None:
    default = autd3.ClientConfig()
    assert default.ack_timeout == autd3.Duration.from_millis(10)
    assert default.max_inflight == 7
    assert default.max_resync_rounds == 8
    assert default.require_supported_firmware is False
    assert default.resynchronize_on_recovery == 3
    assert default.release_failsafe_on_recovery == 3

    config = autd3.ClientConfig(
        ack_timeout=autd3.Duration.from_millis(3),
        max_inflight=2,
        max_resync_rounds=5,
        require_supported_firmware=True,
        resynchronize_on_recovery=None,
        release_failsafe_on_recovery=5,
    )
    assert config.resynchronize_on_recovery is None
    assert config.release_failsafe_on_recovery == 5
    assert config.ack_timeout == autd3.Duration.from_millis(3)
    assert config.max_inflight == 2
    assert config.max_resync_rounds == 5
    assert config.require_supported_firmware is True


def test_the_emulator_option_only_shortens_the_timeouts() -> None:
    emulator = autd3.UdpEmulator(1)
    option = emulator.option()
    default = autd3.UdpOption()
    assert option.iface != default.iface
    assert option.reply_timeout == autd3.Duration.from_millis(50)
    assert option.response_timeout == autd3.Duration.from_millis(50)
    assert option.enumeration_timeout == autd3.Duration.from_secs(1)
    assert option.sync_timeout == autd3.Duration.from_secs(5)
    assert option.heartbeat == default.heartbeat
    assert option.lost_timeout == default.lost_timeout
    assert option.recovery_interval == default.recovery_interval
    assert option.send_rate_limit == default.send_rate_limit
    assert option.send_buffer == default.send_buffer
    assert option.timer_resolution == default.timer_resolution


def test_a_server_link_defaults_to_the_well_known_port() -> None:
    link = autd3.ServerLink()
    assert link.port == 44338
    assert link.timeout == autd3.Duration.from_secs(1)
    assert autd3.ServerLink(port=5000, timeout=autd3.Duration.from_millis(200)) == autd3.ServerLink(
        5000, autd3.Duration.from_millis(200)
    )
    assert autd3.ServerLink(5000) != autd3.ServerLink()
    assert repr(autd3.ServerLink(5000)) == "ServerLink(port=5000)"
    with pytest.raises(ValueError):
        autd3.ServerLink(timeout=autd3.Duration.from_secs(0))
    with pytest.raises(OverflowError):
        autd3.ServerLink(65536)


def test_client_open_takes_a_udp_option_or_a_server_link() -> None:
    with pytest.raises(TypeError):
        autd3.Client.open(geometry(1), loose(autd3.Interface.Auto), autd3.ClientConfig())


def test_a_client_without_a_server_fails_at_once() -> None:
    import socket

    with socket.socket(socket.AF_INET6, socket.SOCK_STREAM) as probe:
        probe.bind(("::1", 0))
        port = probe.getsockname()[1]

    async def open_client() -> None:
        await autd3.Client.open(geometry(1), autd3.ServerLink(port), autd3.ClientConfig())

    with pytest.raises(autd3.Autd3Error, match="no server accepts"):
        asyncio.run(open_client())
