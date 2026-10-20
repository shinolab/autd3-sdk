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
    option = autd3.TransportOption()
    assert option.iface == autd3.Interface.Auto
    assert option.reply_timeout == autd3.Duration.from_millis(1)
    assert option.lost_timeout == autd3.Duration.from_millis(100)
    assert option.response_timeout == autd3.Duration.from_millis(200)
    assert option.enumeration_timeout == autd3.Duration.from_secs(10)
    assert option.sync_timeout == autd3.Duration.from_secs(30)
    assert option.send_rate_limit is None
    assert option.send_buffer == 8192


def test_fields_round_trip() -> None:
    option = autd3.TransportOption(
        iface="eth1",
        lost_timeout=autd3.Duration.from_millis(50),
        sync_timeout=autd3.Duration.from_secs(3),
        send_rate_limit=95.0,
    )
    assert option.iface == autd3.Interface.Name("eth1")
    assert option.lost_timeout == autd3.Duration.from_millis(50)
    assert option.sync_timeout == autd3.Duration.from_secs(3)
    assert option.send_rate_limit == 95.0
    assert autd3.TransportOption(send_buffer=None).send_buffer is None
    assert autd3.TransportOption(send_buffer=65536).send_buffer == 65536


def test_the_interface_selects_where_to_connect() -> None:
    assert autd3.TransportOption().iface == autd3.Interface.Auto
    with pytest.raises(TypeError):
        autd3.TransportOption(iface=loose(None))
    simulator = autd3.TransportOption(iface=autd3.Interface.Simulator(44336))
    assert simulator.iface == autd3.Interface.Simulator(44336)
    assert simulator.iface != autd3.Interface.Simulator(50000)
    assert simulator.iface != autd3.Interface.Auto
    assert autd3.Interface.Simulator(50000).simulator_port() == 50000
    assert autd3.Interface.Auto.simulator_port() is None
    assert repr(autd3.Interface.Simulator(50000)) == "Interface.Simulator(50000)"
    with pytest.raises(OverflowError):
        autd3.Interface.Simulator(65536)
    assert repr(autd3.Interface.Name("eth0")) == 'Interface.Name("eth0")'
    assert autd3.Interface.Name("eth0").name() == "eth0"
    assert autd3.Interface.Auto.name() is None
    assert autd3.Interface.Simulator(44336).name() is None
    assert hash(autd3.Interface.Name("eth0")) == hash(autd3.Interface.Name("eth0"))
    assert len({autd3.Interface.Auto, autd3.Interface.Simulator(44336), autd3.Interface.Auto}) == 2
    with pytest.raises(TypeError):
        autd3.TransportOption(iface=loose(1))


def test_an_address_interface_round_trips_and_rejects_garbage() -> None:
    iface = autd3.Interface.Addr("[::1]:44336")
    assert repr(iface) == 'Interface.Addr("[::1]:44336")'
    assert autd3.TransportOption(iface=iface).iface == iface
    assert iface != autd3.Interface.Simulator(44336)
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

    config = autd3.ClientConfig(
        ack_timeout=autd3.Duration.from_millis(3),
        max_inflight=2,
    )
    assert config.ack_timeout == autd3.Duration.from_millis(3)
    assert config.max_inflight == 2


def test_the_emulator_option_only_shortens_the_timeouts() -> None:
    emulator = autd3.UdpEmulator(1)
    option = emulator.option()
    default = autd3.TransportOption()
    assert option.iface != default.iface
    assert option.reply_timeout == autd3.Duration.from_millis(50)
    assert option.response_timeout == autd3.Duration.from_millis(50)
    assert option.enumeration_timeout == autd3.Duration.from_secs(1)
    assert option.sync_timeout == autd3.Duration.from_secs(5)
    assert option.lost_timeout == default.lost_timeout
    assert option.send_rate_limit == default.send_rate_limit
    assert option.send_buffer == default.send_buffer


def test_client_open_takes_a_transport_option() -> None:
    with pytest.raises(TypeError):
        autd3.Client.open(geometry(1), loose(autd3.Interface.Auto), autd3.ClientConfig())
