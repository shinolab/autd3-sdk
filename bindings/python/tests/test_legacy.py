"""Hardware-free tests: the legacy client driven by the current command types."""

import asyncio

import numpy as np
import pytest

import autd3
import autd3_link_nop as nop
import autd3_modulation as modulation
import autd3_pattern as pattern
from autd3.units import Hz, m, s


def geometry() -> autd3.geometry.Geometry:
    return autd3.geometry.Geometry([autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])


async def open_client() -> autd3.LegacyClient:
    return await autd3.LegacyClient.open(geometry(), nop.Nop(), autd3.LegacyClientConfig())


def test_open_reports_legacy_firmware_version() -> None:
    async def run() -> None:
        client = await open_client()
        assert client.num_devices() == 1
        versions = await client.read_firmware_version()
        assert len(versions) == 1
        assert "legacy-v12.1.0" in versions[0]
        await client.close()

    asyncio.run(run())


def test_current_command_types_drive_the_legacy_client() -> None:
    async def run() -> None:
        geo = geometry()
        client = await autd3.LegacyClient.open(geo, nop.Nop(), autd3.LegacyClientConfig())

        target = geo.center() + np.array([0.0, 0.0, 150.0])
        patterns = geo.pattern_buffer()
        pattern.focus(geo, target, pattern.wavelength(340 * m / s), pattern.FocusOption(), patterns)

        mod_buf = modulation.modulation_buffer()
        modulation.sine(200 * Hz, modulation.SineOption(), mod_buf)

        builder = client.datagram_builder()
        builder.push(autd3.commands.SetSilencer())
        builder.push(autd3.commands.Pattern(patterns))
        builder.push(autd3.commands.Modulation(autd3.value.SamplingConfig.FREQ_4K, mod_buf))
        frames = builder.build()
        assert len(frames) > 0
        for i in range(len(frames)):
            await client.send_checked(frames[i])

        states = await client.read_fpga_state()
        assert len(states) == 1

        await client.stop()
        await client.close()

    asyncio.run(run())


def test_open_with_checker_reports_the_link_status() -> None:
    async def run() -> None:
        client, checker = await autd3.LegacyClient.open_with_checker(
            geometry(), nop.Nop(), autd3.LegacyClientConfig()
        )
        assert client.num_devices() == 1

        status = await checker.check()
        assert len(status.device_states) == 1
        assert status.all_op
        assert not status.any_lost
        assert status.recoveries == 0

        await client.close()

    asyncio.run(run())


def test_push_each_assigns_a_command_per_device() -> None:
    async def run() -> None:
        geo = autd3.geometry.Geometry([
            autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0]),
            autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0]),
        ])
        client = await autd3.LegacyClient.open(geo, nop.Nop(), autd3.LegacyClientConfig())

        wavelength = pattern.wavelength(340 * m / s)
        left = geo.pattern_buffer()
        pattern.focus(geo, geo.center() + np.array([-40.0, 0.0, 150.0]), wavelength, pattern.FocusOption(), left)
        mod_buf = modulation.modulation_buffer()
        modulation.sine(150 * Hz, modulation.SineOption(), mod_buf)

        builder = client.datagram_builder()
        builder.push_each(
            lambda device: autd3.commands.Pattern(left)
            if device.idx() == 0
            else autd3.commands.Modulation(autd3.value.SamplingConfig.FREQ_4K, mod_buf)
        )
        frames = builder.build()
        assert len(frames) > 0
        for i in range(len(frames)):
            await client.send_checked(frames[i])

        # returning None leaves that device unassigned
        builder = client.datagram_builder()
        builder.push_each(lambda device: autd3.commands.Pattern(left) if device.idx() == 0 else None)
        assert len(builder.build()) == 1

        await client.close()

    asyncio.run(run())


def test_push_each_rejects_unsupported_commands_at_build_time() -> None:
    async def run() -> None:
        client = await open_client()
        builder = client.datagram_builder()
        builder.push_each(lambda _device: autd3.commands.ChangePatternBank(autd3.value.PatternBank.B1))
        with pytest.raises(ValueError, match="ChangePatternBank"):
            builder.build()
        await client.close()

    asyncio.run(run())


def test_unsupported_commands_are_rejected_at_build_time() -> None:
    async def run() -> None:
        client = await open_client()
        builder = client.datagram_builder()
        builder.push(autd3.commands.ChangePatternBank(autd3.value.PatternBank.B1))
        with pytest.raises(ValueError, match="ChangePatternBank"):
            builder.build()
        await client.close()

    asyncio.run(run())


def test_change_segment_switches_the_pattern_bank() -> None:
    async def run() -> None:
        client = await open_client()
        for cmd in (
            autd3.LegacyChangePatternBank.pattern(autd3.value.PatternBank.B1),
            autd3.LegacyChangePatternBank.foci_stm(autd3.value.PatternBank.B0),
            autd3.LegacyChangePatternBank.foci_stm(autd3.value.PatternBank.B1, autd3.value.TransitionMode.SyncIdx),
            autd3.LegacyChangePatternBank.pattern_stm(autd3.value.PatternBank.B0),
            autd3.LegacyChangePatternBank.pattern_stm(autd3.value.PatternBank.B1, autd3.value.TransitionMode.Ext),
        ):
            builder = client.datagram_builder()
            builder.push(cmd)
            frames = builder.build()
            assert len(frames) == 1
        builder = client.datagram_builder()
        builder.push(autd3.LegacyChangePatternBank.pattern(autd3.value.PatternBank.B0))
        await client.send_checked(builder.build()[0])
        await client.close()

    asyncio.run(run())


def test_change_segment_is_legacy_only() -> None:
    async def run() -> None:
        client = await autd3.Client.open(geometry(), nop.Nop(), autd3.ClientConfig())
        builder = client.datagram_builder()
        with pytest.raises(ValueError, match="Unknown datagram type"):
            builder.push(autd3.LegacyChangePatternBank.pattern(autd3.value.PatternBank.B1))
        await client.close()

    asyncio.run(run())


def test_later_stages_a_modulation_bank_then_change_bank_switches_it() -> None:
    async def run() -> None:
        client = await open_client()
        buf = modulation.modulation_buffer()
        modulation.sine(200 * Hz, modulation.SineOption(), buf)

        builder = client.datagram_builder()
        builder.push(
            autd3.commands.Modulation(
                autd3.value.SamplingConfig.FREQ_4K,
                buf,
                bank=autd3.value.ModulationBank.B1,
                transition_mode=autd3.value.TransitionMode.Later,
            )
        )
        builder.push(autd3.commands.ChangeModulationBank(autd3.value.ModulationBank.B1))
        frames = builder.build()
        assert len(frames) > 0
        for frame in frames:
            await client.send_checked(frame)
        await client.close()

    asyncio.run(run())


def test_a_legacy_bank_change_refuses_to_not_transition() -> None:
    async def run() -> None:
        client = await open_client()
        builder = client.datagram_builder()
        builder.push(
            autd3.commands.ChangeModulationBank(
                autd3.value.ModulationBank.B1,
                transition_mode=autd3.value.TransitionMode.Later,
            )
        )
        with pytest.raises(autd3.Autd3Error, match="Later"):
            builder.build()
        await client.close()

    asyncio.run(run())


def test_zero_timeout_cycles_is_rejected() -> None:
    with pytest.raises(ValueError, match="timeout_cycles must be >= 1"):
        autd3.LegacyClientConfig(timeout_cycles=0)
    assert autd3.LegacyClientConfig(timeout_cycles=1) is not None


def test_send_returns_one_response_byte_per_device() -> None:
    async def run() -> None:
        geo = autd3.geometry.Geometry([
            autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0]),
            autd3.geometry.Autd3([200.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0]),
        ])
        client = await autd3.LegacyClient.open(geo, nop.Nop(), autd3.LegacyClientConfig(timeout_cycles=200))
        assert client.num_devices() == 2

        builder = client.datagram_builder()
        builder.push(autd3.commands.ForceFan(value=True))
        frames = builder.build()
        assert len(frames) == 1

        response = await client.send(frames[0])
        assert len(response) == 2

        await client.close()

    asyncio.run(run())


def test_every_unsupported_command_is_rejected_at_build_time() -> None:
    async def run() -> None:
        geo = geometry()
        client = await autd3.LegacyClient.open(geo, nop.Nop(), autd3.LegacyClientConfig())

        buf = geo.pattern_buffer()
        pattern.focus(geo, geo.center() + np.array([0.0, 0.0, 150.0]), pattern.wavelength(340 * m / s),
                      pattern.FocusOption(), buf)
        mod_buf = modulation.modulation_buffer()
        modulation.sine(200 * Hz, modulation.SineOption(), mod_buf)
        points = [
            autd3.value.ControlPoints([autd3.value.ControlPoint([0.0, 0.0, 140.0 + i])]) for i in range(2)
        ]

        unsupported = (
            ("WritePatternBuffer", autd3.commands.WritePatternBuffer(autd3.value.PatternBank.B1, 0, buf)),
            ("WriteFociBuffer", autd3.commands.WriteFociBuffer(autd3.value.PatternBank.B1, 0, points)),
            ("ConfigPattern", autd3.commands.ConfigPattern(
                autd3.value.PatternBank.B1, autd3.value.SamplingConfig.FREQ_4K, 2)),
            ("ConfigFociStm", autd3.commands.ConfigFociStm(
                autd3.value.PatternBank.B1, autd3.value.SamplingConfig.FREQ_4K, 2, 1, 340 * m / s)),
            ("WritePatternCompressed", autd3.commands.WritePatternCompressed(
                autd3.value.PatternBank.B1, 0, autd3.commands.PatternCompression.PhaseFull, [buf, buf])),
            ("ChangePatternBank", autd3.commands.ChangePatternBank(autd3.value.PatternBank.B1)),
            ("WriteModulationBuffer", autd3.commands.WriteModulationBuffer(
                autd3.value.ModulationBank.B1, 0, mod_buf)),
            ("ConfigModulation", autd3.commands.ConfigModulation(
                autd3.value.ModulationBank.B1, autd3.value.SamplingConfig.FREQ_4K, 2)),
        )
        for name, cmd in unsupported:
            builder = client.datagram_builder()
            builder.push(cmd)
            with pytest.raises(ValueError, match=name):
                builder.build()

        await client.close()

    asyncio.run(run())


def test_a_link_without_legacy_support_asks_for_a_wheel_update() -> None:
    class OldLink:
        """A link wheel predating LegacyClient: it only exposes the current capsule."""

        def _capsule(self) -> object:
            raise NotImplementedError

    async def run() -> None:
        with pytest.raises(TypeError, match="update the autd3-link-"):
            await autd3.LegacyClient.open(geometry(), OldLink(), autd3.LegacyClientConfig())
        with pytest.raises(TypeError, match="update the autd3-link-"):
            await autd3.LegacyClient.open_with_checker(geometry(), OldLink(), autd3.LegacyClientConfig())

    asyncio.run(run())
