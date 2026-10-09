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


def test_the_emulator_option_points_at_the_emulator() -> None:
    emulator = autd3.UdpEmulator(1)
    assert repr(emulator.option().iface).startswith('Interface.Addr("[::1]:')


def test_an_address_interface_round_trips_and_rejects_garbage() -> None:
    iface = autd3.Interface.Addr("[::1]:44336")
    assert repr(iface) == 'Interface.Addr("[::1]:44336")'
    assert autd3.TransportOption(iface=iface).iface == iface
    assert iface != autd3.Interface.Simulator
    with pytest.raises(ValueError):
        autd3.Interface.Addr("127.0.0.1:1")


def test_the_client_opens_the_emulated_chain() -> None:
    emulator = autd3.UdpEmulator(2)
    assert emulator.num_devices == 2

    async def run() -> None:
        client = await autd3.Client.open(geometry(2), emulator.option(), autd3.ClientConfig())
        checker = client.state_checker()
        async with client:
            assert client.num_devices() == 2
            assert client.device_time_now().sys_time < 60_000_000_000
            versions = await client.read_firmware_version()
            assert len(versions) == 2
            for version in versions:
                assert isinstance(version, autd3.FirmwareVersion)
                assert version.is_emulator()
                assert version.is_supported()
                assert (version.cpu.major, version.cpu.minor) == autd3.FirmwareVersion.SUPPORTED_SERIES
                assert (version.fpga.major, version.fpga.minor) == autd3.FirmwareVersion.SUPPORTED_SERIES
                assert not version.fpga.is_unknown()
                assert str(version) == f"CPU: {version.cpu}, FPGA: {version.fpga} [Emulator]"
            assert versions[0] == versions[1]
            assert hash(versions[0]) == hash(versions[1])
            status = checker.check()
            assert status.all_ready
            assert not status.any_lost
            assert status.devices == [autd3.DeviceState.Ready, autd3.DeviceState.Ready]
            assert [str(state) for state in status.devices] == ["READY", "READY"]
            assert status == checker.check()
            for state in await client.read_fpga_state():
                assert state.current_mod_bank() == autd3.value.ModulationBank.B0
                assert state.current_pattern_bank() == autd3.value.PatternBank.B0
                assert state.is_pattern_mode() != state.is_stm_mode()
            stats = client.bus_stats()
            assert isinstance(stats, autd3.BusStats)
            frames = stats.frames()
            assert frames > 0
            assert stats.acked_frames() > 0
            assert stats.mean_ack_latency_ns() <= stats.worst_ack_latency_ns()
            assert stats.missed_replies() >= 0
            assert stats.resets() >= 0
            assert stats.heartbeats() >= 0
            await client.send(autd3.commands.Nop())
            assert stats.frames() > frames
            telemetry = await client.read_telemetry()
            assert len(telemetry) == 2
            for counters in telemetry:
                assert isinstance(counters, autd3.TelemetryCounters)
                assert len(counters.as_list()) == len(autd3.value.Telemetry.ALL)
                assert counters[autd3.value.Telemetry.Failsafe] == counters.get(
                    autd3.value.Telemetry.Failsafe
                )
        with pytest.raises(autd3_core.Autd3Error):
            checker.check()

    asyncio.run(run())


def test_a_device_count_mismatch_fails_to_open() -> None:
    emulator = autd3.UdpEmulator(1)

    async def run() -> None:
        await autd3.Client.open(geometry(2), emulator.option(), autd3.ClientConfig())

    with pytest.raises(autd3_core.Autd3Error):
        asyncio.run(run())


def test_device_states_are_values() -> None:
    assert autd3.DeviceState.Ready == autd3.DeviceState.Ready
    assert autd3.DeviceState.Ready != autd3.DeviceState.Lost
    assert len({autd3.DeviceState.Ready, autd3.DeviceState.Syncing, autd3.DeviceState.Lost}) == 3
    assert repr(autd3.DeviceState.Syncing) == "DeviceState.Syncing"
    assert str(autd3.DeviceState.Lost) == "LOST"


def test_errors_carry_a_code() -> None:
    assert autd3.Autd3Error("plain").code == autd3.Autd3Error.GENERIC
    codes = [
        autd3.Autd3Error.GENERIC,
        autd3.Autd3Error.TIMEOUT,
        autd3.Autd3Error.DEVICE,
        autd3.Autd3Error.NETWORK,
        autd3.Autd3Error.INVALID_ARGUMENT,
        autd3.Autd3Error.UNSUPPORTED_FIRMWARE,
    ]
    assert codes == [-1, -2, -3, -4, -5, -6]

    emulator = autd3.UdpEmulator(1)
    invalid = autd3.commands.SetCpuConfig(autd3.commands.CpuConfig(failsafe_timeout=autd3.Duration.from_nanos(0)))
    rejected = autd3.commands.SetSilencer(
        autd3.commands.FixedCompletionTime(
            intensity=autd3.Duration.from_micros(500),
            phase=autd3.Duration.from_micros(1000),
            strict_mode=True,
        )
    )
    buf = autd3_modulation.modulation_buffer()
    autd3_modulation.sine(150 * autd3.units.Hz, autd3_modulation.SineOption(), buf)
    modulation = autd3.commands.Modulation(autd3.value.SamplingConfig.FREQ_4K, buf)

    async def run() -> None:
        client = await autd3.Client.open(geometry(1), emulator.option(), autd3.ClientConfig())
        with pytest.raises(autd3_core.Autd3Error) as invalid_argument:
            await client.send(invalid)
        assert invalid_argument.value.code == autd3.Autd3Error.INVALID_ARGUMENT
        with pytest.raises(autd3_core.Autd3Error) as device:
            await client.send((modulation, rejected))
        assert device.value.code == autd3.Autd3Error.DEVICE
        await client.close()
        with pytest.raises(autd3_core.Autd3Error) as closed:
            await client.read_firmware_version()
        assert closed.value.code == autd3.Autd3Error.GENERIC

    asyncio.run(run())

    with pytest.raises(autd3_core.Autd3Error) as encode:
        autd3.Frames.encode(geometry(1), invalid)
    assert encode.value.code == autd3.Autd3Error.INVALID_ARGUMENT


def test_an_emulator_that_fails_to_start_raises_the_binding_error() -> None:
    if sys.platform != "linux":
        pytest.skip("the descriptor limit is only lowered on Linux")
    import resource

    soft, hard = resource.getrlimit(resource.RLIMIT_NOFILE)
    resource.setrlimit(resource.RLIMIT_NOFILE, (3, hard))
    try:
        with pytest.raises(autd3_core.Autd3Error) as failure:
            autd3.UdpEmulator(1)
    finally:
        resource.setrlimit(resource.RLIMIT_NOFILE, (soft, hard))
    assert failure.value.code == autd3.Autd3Error.NETWORK


def test_reboot_rejects_an_out_of_range_index() -> None:
    emulator = autd3.UdpEmulator(1)
    with pytest.raises(IndexError):
        emulator.reboot(1)
