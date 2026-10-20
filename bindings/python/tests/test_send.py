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


async def processed(client: autd3.Client) -> int:
    return (await client.read_telemetry())[0].processed


def test_each_asks_every_device_and_pads_the_shorter_ones() -> None:
    geo = geometry()
    seen: list[int] = []

    def assign(device: autd3.geometry.Device) -> autd3.commands.Command | None:
        seen.append(device.idx())
        return sine() if device.idx() == 0 else None

    command = each(assign)
    assert seen == []
    frames = autd3.Frames.encode(geo, command)
    assert seen == [0, 1]
    assert len(frames) == len(autd3.Frames.encode(geo, sine()))

    mixed = each(lambda device: sine() if device.idx() == 0 else autd3.commands.Nop())
    assert len(autd3.Frames.encode(geo, mixed)) == len(frames)

    assert len(autd3.Frames.encode(geo, each(lambda _: None))) == 0
    assert len(autd3.Frames.encode(geo, each(lambda _: (autd3.commands.Nop(), autd3.commands.Nop())))) == 2
    assert len(autd3.Frames.encode(geo, each(lambda _: each(lambda _: autd3.commands.Nop())))) == 1


def test_each_rejects_what_is_not_a_command() -> None:
    geo = geometry()
    with pytest.raises(TypeError):
        each(loose(1))
    with pytest.raises(TypeError):
        autd3.Frames.encode(geo, each(lambda _: loose(1)))

    class Marker(Exception):
        pass

    def failing(_: autd3.geometry.Device) -> autd3.commands.Command | None:
        raise Marker

    with pytest.raises(Marker):
        autd3.Frames.encode(geo, each(failing))


def test_a_tuple_or_a_list_expands_its_commands_in_order() -> None:
    geo = geometry()
    nop = autd3.commands.Nop()
    single = len(autd3.Frames.encode(geo, sine()))

    assert len(autd3.Frames.encode(geo, (nop, sine()))) == 1 + single
    assert len(autd3.Frames.encode(geo, [nop, sine()])) == 1 + single
    assert len(autd3.Frames.encode(geo, [nop, (sine(), [nop, nop]), each(lambda _: nop)])) == 4 + single
    assert len(autd3.Frames.encode(geo, ())) == 0
    assert len(autd3.Frames.encode(geo, [])) == 0

    with pytest.raises(TypeError):
        autd3.Frames.encode(geo, loose(1))
    with pytest.raises(TypeError):
        autd3.Frames.encode(geo, loose((nop, "nop")))
    with pytest.raises(TypeError):
        autd3.Frames.encode(geo, loose(iter([nop])))


def test_a_self_referential_sequence_is_a_recursion_error() -> None:
    cyclic: list[object] = []
    cyclic.append(cyclic)

    with pytest.raises(RecursionError):
        autd3.Frames.encode(geometry(), loose(cyclic))
