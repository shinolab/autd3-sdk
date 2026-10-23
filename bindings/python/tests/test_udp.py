import inspect
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
    assert option.open_timeout == autd3.Duration.from_secs(40)
    assert option.lost_timeout == autd3.Duration.from_millis(100)
    assert option.ack_timeout == autd3.Duration.from_millis(10)
    assert option.send_buffer_bytes == 8192
    assert option.sync_on_open is True


def test_fields_round_trip() -> None:
    option = autd3.TransportOption(
        iface="eth1",
        lost_timeout=autd3.Duration.from_millis(50),
        open_timeout=autd3.Duration.from_secs(3),
        ack_timeout=autd3.Duration.from_millis(3),
    )
    assert option.iface == autd3.Interface.Name("eth1")
    assert option.lost_timeout == autd3.Duration.from_millis(50)
    assert option.open_timeout == autd3.Duration.from_secs(3)
    assert option.ack_timeout == autd3.Duration.from_millis(3)
    assert autd3.TransportOption(send_buffer_bytes=None).send_buffer_bytes is None
    assert autd3.TransportOption(send_buffer_bytes=65536).send_buffer_bytes == 65536


def test_sync_on_open_can_be_turned_off() -> None:
    option = autd3.TransportOption(sync_on_open=False)
    assert option.sync_on_open is False
    assert option != autd3.TransportOption()
    assert "sync_on_open: false" in repr(option)


def test_the_interface_selects_where_to_connect() -> None:
    assert autd3.TransportOption().iface == autd3.Interface.Auto
    with pytest.raises(TypeError):
        autd3.TransportOption(iface=loose(None))
    named = autd3.TransportOption(iface=autd3.Interface.Name("eth0"))
    assert named.iface == autd3.Interface.Name("eth0")
    assert named.iface != autd3.Interface.Name("eth1")
    assert named.iface != autd3.Interface.Auto
    assert repr(autd3.Interface.Name("eth0")) == 'Interface.Name("eth0")'
    assert autd3.Interface.Name("eth0").name() == "eth0"
    assert autd3.Interface.Auto.name() is None
    assert autd3.Interface.Addr("[::1]:44336").name() is None
    assert hash(autd3.Interface.Name("eth0")) == hash(autd3.Interface.Name("eth0"))
    assert len({autd3.Interface.Auto, autd3.Interface.Name("eth0"), autd3.Interface.Auto}) == 2
    with pytest.raises(TypeError):
        autd3.TransportOption(iface=loose(1))


def test_an_address_interface_round_trips_and_rejects_garbage() -> None:
    iface = autd3.Interface.Addr("[::1]:44336")
    assert repr(iface) == 'Interface.Addr("[::1]:44336")'
    assert autd3.TransportOption(iface=iface).iface == iface
    assert iface != autd3.Interface.Addr("[::1]:50000")
    with pytest.raises(ValueError):
        autd3.Interface.Addr("127.0.0.1:1")


def test_device_states_are_values() -> None:
    assert autd3.DeviceState.Ready == autd3.DeviceState.Ready
    assert autd3.DeviceState.Ready != autd3.DeviceState.Lost
    assert len({autd3.DeviceState.Ready, autd3.DeviceState.Syncing, autd3.DeviceState.Lost}) == 3
    assert repr(autd3.DeviceState.Syncing) == "DeviceState.Syncing"
    assert str(autd3.DeviceState.Lost) == "LOST"


def test_device_status_is_read_from_the_client_per_device() -> None:
    assert callable(getattr(autd3.Client, "device_status", None))
    for removed in ("state_checker", "bus_stats"):
        assert not hasattr(autd3.Client, removed)
    for removed in ("StateChecker", "BusStats", "DeviceMask"):
        assert not hasattr(autd3, removed)
    for field in ("state", "ptp_locked", "ptp_failsafe"):
        assert inspect.isdatadescriptor(getattr(autd3.DeviceStatus, field))
    for removed in ("devices", "all_ready", "any_lost", "any_ptp_failsafe"):
        assert not hasattr(autd3.DeviceStatus, removed)
    with pytest.raises(TypeError):
        loose(autd3.DeviceStatus)()


def test_fpga_state_has_no_stm_mode_query() -> None:
    assert callable(getattr(autd3.FpgaState, "is_pattern_mode", None))
    assert not hasattr(autd3.FpgaState, "is_stm_mode")


def test_a_zero_ack_timeout_is_constructible_and_a_non_duration_is_rejected() -> None:
    assert autd3.TransportOption(ack_timeout=autd3.Duration.from_millis(0)).ack_timeout == autd3.Duration.from_millis(0)
    with pytest.raises(TypeError):
        autd3.TransportOption(ack_timeout=loose(None))


def test_client_open_takes_a_transport_option() -> None:
    with pytest.raises(TypeError):
        autd3.Client.open(geometry(1), loose(autd3.Interface.Auto))


def test_client_is_a_synchronous_context_manager() -> None:
    assert callable(getattr(autd3.Client, "__enter__", None))
    assert callable(getattr(autd3.Client, "__exit__", None))
    assert not hasattr(autd3.Client, "__aenter__")
    assert not hasattr(autd3.Client, "__aexit__")
