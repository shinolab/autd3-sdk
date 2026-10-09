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
    option = autd3.TransportOption()
    assert option.iface == autd3.Interface.Auto
    assert option.heartbeat == autd3.Duration.from_millis(10)
    assert option.reply_timeout == autd3.Duration.from_millis(1)
    assert option.lost_timeout == autd3.Duration.from_millis(100)
    assert option.response_timeout == autd3.Duration.from_millis(200)
    assert option.enumeration_timeout == autd3.Duration.from_secs(10)
    assert option.sync_timeout == autd3.Duration.from_secs(30)
    assert option.send_rate_limit is None
    assert option.send_buffer == 8192
    assert option.timer_resolution == autd3.Duration.from_millis(1)


def test_fields_round_trip() -> None:
    option = autd3.TransportOption(
        iface="eth1",
        heartbeat=autd3.Duration.from_millis(2),
        lost_timeout=autd3.Duration.from_millis(50),
        sync_timeout=autd3.Duration.from_secs(3),
        send_rate_limit=95.0,
    )
    assert option.iface == autd3.Interface.Name("eth1")
    assert option.heartbeat == autd3.Duration.from_millis(2)
    assert autd3.TransportOption(heartbeat=None).heartbeat is None
    assert autd3.TransportOption(iface="eth0").heartbeat == autd3.Duration.from_millis(10)
    assert option.lost_timeout == autd3.Duration.from_millis(50)
    assert option.sync_timeout == autd3.Duration.from_secs(3)
    assert option.send_rate_limit == 95.0
    assert autd3.TransportOption(timer_resolution=None).timer_resolution is None
    assert autd3.TransportOption(send_buffer=None).send_buffer is None
    assert autd3.TransportOption(send_buffer=65536).send_buffer == 65536
    assert autd3.TransportOption(
        timer_resolution=autd3.Duration.from_millis(2)
    ).timer_resolution == autd3.Duration.from_millis(2)


def test_the_interface_selects_where_to_connect() -> None:
    assert autd3.TransportOption(iface=None).iface == autd3.Interface.Auto
    simulator = autd3.TransportOption(iface=autd3.Interface.Simulator)
    assert simulator.iface == autd3.Interface.Simulator
    assert simulator.iface != autd3.Interface.Auto
    assert repr(autd3.Interface.Name("eth0")) == 'Interface.Name("eth0")'
    assert autd3.Interface.Name("eth0").name() == "eth0"
    assert autd3.Interface.Auto.name() is None
    assert autd3.Interface.Simulator.name() is None
    assert hash(autd3.Interface.Name("eth0")) == hash(autd3.Interface.Name("eth0"))
    assert len({autd3.Interface.Auto, autd3.Interface.Simulator, autd3.Interface.Auto}) == 2
    with pytest.raises(TypeError):
        autd3.TransportOption(iface=loose(1))


def test_an_address_interface_round_trips_and_rejects_garbage() -> None:
    iface = autd3.Interface.Addr("[::1]:44336")
    assert repr(iface) == 'Interface.Addr("[::1]:44336")'
    assert autd3.TransportOption(iface=iface).iface == iface
    assert iface != autd3.Interface.Simulator
    with pytest.raises(ValueError):
        autd3.Interface.Addr("127.0.0.1:1")


def test_device_states_are_values() -> None:
    assert autd3.DeviceState.Ready == autd3.DeviceState.Ready
    assert autd3.DeviceState.Ready != autd3.DeviceState.Lost
    assert len({autd3.DeviceState.Ready, autd3.DeviceState.Syncing, autd3.DeviceState.Lost}) == 3
    assert repr(autd3.DeviceState.Syncing) == "DeviceState.Syncing"
    assert str(autd3.DeviceState.Lost) == "LOST"
