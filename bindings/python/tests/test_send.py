import asyncio
from collections.abc import Awaitable, Callable
from typing import Any

import pytest

import autd3
import autd3_core
import autd3_modulation as modulation
from autd3.commands import each
from autd3.units import Hz

DEVICES = 2
TRAILING = 3


def loose(value: object) -> Any:
    return value


async def result_of(awaitable: Awaitable[object]) -> object:
    return await awaitable


def geometry(n: int = DEVICES) -> autd3.geometry.Geometry:
    return autd3.geometry.Geometry(
        [autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0]) for _ in range(n)]
    )


def sine() -> autd3.commands.Modulation:
    buf = modulation.modulation_buffer()
    modulation.sine(150 * Hz, modulation.SineOption(), autd3.value.SamplingConfig.FREQ_4K, buf)
    return autd3.commands.Modulation(autd3.value.SamplingConfig.FREQ_4K, buf)


def rejected_silencer() -> autd3.commands.SetSilencer:
    return autd3.commands.SetSilencer(
        autd3.commands.FixedCompletionTime(
            intensity=autd3.Duration.from_micros(500),
            phase=autd3.Duration.from_micros(1000),
            strict_mode=True,
        )
    )


def rejected_in_the_middle() -> list[autd3.commands.Command]:
    return [sine(), rejected_silencer(), *[autd3.commands.ForceFan(True) for _ in range(TRAILING)]]


def test_each_asks_every_device_and_leaves_the_unassigned_ones_out() -> None:
    geo = geometry()
    seen: list[int] = []

    def assign(device: autd3.geometry.Device) -> autd3.commands.Operation | None:
        seen.append(device.idx())
        return autd3.commands.ForceFan(True) if device.idx() == 0 else None

    command = each(assign)
    assert seen == []
    frames = autd3.Frames.encode(geo, command)
    assert seen == [0, 1]
    assert len(frames) == 1

    mixed = each(lambda device: autd3.commands.ForceFan(True) if device.idx() == 0 else autd3.commands.Nop())
    assert len(autd3.Frames.encode(geo, mixed)) == 1

    assert len(autd3.Frames.encode(geo, each(lambda _: None))) == 0


def test_each_takes_multi_frame_commands_and_pads_the_shorter_devices() -> None:
    geo = geometry(3)
    alone = len(autd3.Frames.encode(geo, sine()))
    nop = autd3.commands.Nop()

    def assign(device: autd3.geometry.Device) -> autd3.commands.Command | None:
        commands: list[autd3.commands.Command] = [sine(), sine(), nop]
        return commands[device.idx()]

    assert len(autd3.Frames.encode(geo, each(assign))) == alone
    assert len(autd3.Frames.encode(geo, each(lambda device: [nop, nop] if device.idx() == 0 else None))) == 2
    nested = each(lambda device: each(lambda _: nop) if device.idx() == 1 else None)
    assert len(autd3.Frames.encode(geo, nested)) == 1


def test_each_takes_the_buffer_writes() -> None:
    geo = geometry()
    data = modulation.ModulationBuffer(4)
    points = [autd3.value.ControlPoints([autd3.value.ControlPoint([0.0, 0.0, 150.0], autd3.value.Phase.ZERO)], 0xFF)]
    write_modulation = each(lambda _: autd3.commands.WriteModulationBuffer(autd3.value.ModulationBank.B0, 0, data))
    write_foci = each(
        lambda device: autd3.commands.WriteFociBuffer(autd3.value.PatternBank.B0, 0, points) if device.idx() == 1 else None
    )
    assert len(autd3.Frames.encode(geo, write_modulation)) == 1
    assert len(autd3.Frames.encode(geo, write_foci)) == 1


def test_buffer_writes_longer_than_one_frame_are_split_into_frames() -> None:
    geo = geometry()
    bank = autd3.value.ModulationBank.B0
    data = modulation.ModulationBuffer(4000)
    assert len(autd3.Frames.encode(geo, autd3.commands.WriteModulationBuffer(bank, 0, data))) > 1

    point = autd3.value.ControlPoint([0.0, 0.0, 150.0], autd3.value.Phase.ZERO)
    points = [autd3.value.ControlPoints([point, point], 0xFF) for _ in range(200)]
    pattern_bank = autd3.value.PatternBank.B0
    assert len(autd3.Frames.encode(geo, autd3.commands.WriteFociBuffer(pattern_bank, 0, points))) > 1


def test_a_pattern_buffer_write_puts_two_consecutive_indices_in_one_frame() -> None:
    geo = geometry()
    phases = geo.phase_buffer()
    intensities = geo.intensity_buffer()
    bank = autd3.value.PatternBank.B0
    for given in (autd3.value.Intensity.MAX, 0x80, intensities, [intensities] * 3):
        write = autd3.commands.WritePatternBuffer(bank, 1, [phases] * 2, given)
        assert len(autd3.Frames.encode(geo, write)) == 1
    assert len(autd3.Frames.encode(geo, each(lambda _: autd3.commands.WritePatternBuffer(bank, 0, [phases], 0xFF)))) == 1
    assert len(autd3.Frames.encode(geo, autd3.commands.WritePatternBuffer(bank, 0, [phases] * 3, 0xFF))) == 2


def test_each_rejects_what_is_not_a_command() -> None:
    geo = geometry()
    with pytest.raises(TypeError):
        each(loose(1))
    with pytest.raises(TypeError):
        autd3.Frames.encode(geo, each(lambda _: loose(1)))

    class Marker(Exception):
        pass

    def failing(_: autd3.geometry.Device) -> autd3.commands.Operation | None:
        raise Marker

    with pytest.raises(Marker):
        autd3.Frames.encode(geo, each(failing))


def test_a_list_expands_its_commands_in_order() -> None:
    geo = geometry()
    nop = autd3.commands.Nop()
    single = len(autd3.Frames.encode(geo, sine()))

    assert len(autd3.Frames.encode(geo, [nop, sine()])) == 1 + single
    assert len(autd3.Frames.encode(geo, [nop, [sine(), [nop, nop]], each(lambda _: nop)])) == 4 + single
    assert len(autd3.Frames.encode(geo, [])) == 0


def test_anything_but_a_command_each_or_a_list_is_rejected() -> None:
    geo = geometry()
    nop = autd3.commands.Nop()
    for command in ((nop, nop), (), 1, iter([nop]), [nop, "nop"]):
        with pytest.raises(TypeError, match="expected a command, each"):
            autd3.Frames.encode(geo, loose(command))


def test_a_self_referential_list_is_a_recursion_error() -> None:
    cyclic: list[object] = []
    cyclic.append(cyclic)

    with pytest.raises(RecursionError):
        autd3.Frames.encode(geometry(), loose(cyclic))


def test_a_self_referential_each_is_a_recursion_error() -> None:
    cyclic: autd3.commands.Each = each(lambda _: loose(cyclic))

    with pytest.raises(RecursionError):
        autd3.Frames.encode(geometry(), cyclic)
