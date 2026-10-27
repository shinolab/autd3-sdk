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


def test_each_rejects_what_is_not_a_single_frame_operation() -> None:
    geo = geometry()
    nop = autd3.commands.Nop()
    not_single_frame: list[Any] = [sine(), each(lambda _: nop)]
    for command in not_single_frame:

        def assign(_: autd3.geometry.Device, command: Any = command) -> Any:
            return command

        with pytest.raises(TypeError, match="single-frame operation for device 0"):
            autd3.Frames.encode(geo, each(assign))


def test_each_takes_the_buffer_writes_as_single_frame_operations() -> None:
    geo = geometry()
    data = modulation.ModulationBuffer(4)
    points = [autd3.value.ControlPoints([autd3.value.ControlPoint([0.0, 0.0, 150.0], autd3.value.Phase.ZERO)], 0xFF)]
    write_modulation = each(lambda _: autd3.commands.WriteModulationBuffer(autd3.value.ModulationBank.B0, 0, data))
    write_foci = each(
        lambda device: autd3.commands.WriteFociBuffer(autd3.value.PatternBank.B0, 0, points) if device.idx() == 1 else None
    )
    assert len(autd3.Frames.encode(geo, write_modulation)) == 1
    assert len(autd3.Frames.encode(geo, write_foci)) == 1


def test_buffer_writes_longer_than_one_frame_are_rejected_when_encoded() -> None:
    geo = geometry()
    max_len = autd3.commands.WriteModulationBuffer.MAX_LEN
    fits = modulation.ModulationBuffer(max_len)
    too_long = modulation.ModulationBuffer(max_len + 1)
    bank = autd3.value.ModulationBank.B0
    assert len(autd3.Frames.encode(geo, autd3.commands.WriteModulationBuffer(bank, 0, fits))) == 1
    with pytest.raises(autd3.Autd3Error, match="fit in one frame") as err:
        autd3.Frames.encode(geo, autd3.commands.WriteModulationBuffer(bank, 0, too_long))
    assert err.value.code == autd3.Autd3Error.INVALID_ARGUMENT

    point = autd3.value.ControlPoint([0.0, 0.0, 150.0], autd3.value.Phase.ZERO)
    foci_max_len = autd3.commands.WriteFociBuffer.max_len(2)
    points = [autd3.value.ControlPoints([point, point], 0xFF) for _ in range(foci_max_len + 1)]
    pattern_bank = autd3.value.PatternBank.B0
    assert len(autd3.Frames.encode(geo, autd3.commands.WriteFociBuffer(pattern_bank, 0, points[:-1]))) == 1
    with pytest.raises(autd3.Autd3Error, match="fit in one frame"):
        autd3.Frames.encode(geo, autd3.commands.WriteFociBuffer(pattern_bank, 0, points))


def test_a_pattern_buffer_write_takes_up_to_max_len_consecutive_indices() -> None:
    geo = geometry()
    phases = geo.phase_buffer()
    intensities = geo.intensity_buffer()
    bank = autd3.value.PatternBank.B0
    max_len = autd3.commands.WritePatternBuffer.MAX_LEN
    assert max_len == 2
    for given in (autd3.value.Intensity.MAX, 0x80, intensities, [intensities] * (1 + max_len)):
        write = autd3.commands.WritePatternBuffer(bank, 1, [phases] * max_len, given)
        assert len(autd3.Frames.encode(geo, write)) == 1
    assert len(autd3.Frames.encode(geo, each(lambda _: autd3.commands.WritePatternBuffer(bank, 0, [phases], 0xFF)))) == 1
    with pytest.raises(autd3.Autd3Error, match="do not fit one frame") as err:
        autd3.Frames.encode(geo, autd3.commands.WritePatternBuffer(bank, 0, [phases] * (max_len + 1), 0xFF))
    assert err.value.code == autd3.Autd3Error.INVALID_ARGUMENT


def test_the_write_foci_buffer_max_len_shrinks_with_the_number_of_foci() -> None:
    assert autd3.commands.WriteFociBuffer.max_len(1) > autd3.commands.WriteFociBuffer.max_len(8) > 0
    for num_foci in (0, 9):
        with pytest.raises(ValueError):
            autd3.commands.WriteFociBuffer.max_len(num_foci)


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


def test_a_tuple_or_a_list_of_commands_is_not_a_command() -> None:
    geo = geometry()
    nop = autd3.commands.Nop()
    for command in ((nop, nop), [nop], (), [], 1, iter([nop])):
        with pytest.raises(TypeError, match="expected a command or each"):
            autd3.Frames.encode(geo, loose(command))


def test_a_self_referential_each_is_a_recursion_error() -> None:
    cyclic: autd3.commands.Each = each(lambda _: loose(cyclic))

    with pytest.raises(RecursionError):
        autd3.Frames.encode(geometry(), cyclic)
