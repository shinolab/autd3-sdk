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


def with_client(body: Callable[[autd3.Client], Awaitable[None]]) -> None:
    async def run() -> None:
        emulator = autd3.UdpEmulator(DEVICES)
        async with await autd3.Client.open(geometry(), emulator.option(), autd3.ClientConfig()) as client:
            await body(client)

    asyncio.run(run())


def sine() -> autd3.commands.Modulation:
    buf = modulation.modulation_buffer()
    modulation.sine(150 * Hz, modulation.SineOption(), buf)
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
    return (await client.read_telemetry())[0][autd3.value.Telemetry.Processed]


def test_send_completes_every_frame_and_returns_none() -> None:
    async def body(client: autd3.Client) -> None:
        before = await processed(client)
        command = sine()
        assert await result_of(client.send(command)) is None
        frames = len(autd3.Frames.encode(client.geometry(), command))
        assert frames > 1
        assert await processed(client) - before == frames + 1

    with_client(body)


def test_send_stops_at_the_frame_the_device_rejected() -> None:
    counts: dict[str, int] = {}

    async def stop_and_wait(client: autd3.Client) -> None:
        before = await processed(client)
        with pytest.raises(autd3_core.Autd3Error):
            await client.send(rejected_in_the_middle())
        counts["send"] = await processed(client) - before

    async def streaming(client: autd3.Client) -> None:
        before = await processed(client)
        done = await client.send_streaming(rejected_in_the_middle())
        with pytest.raises(autd3_core.Autd3Error):
            await done
        counts["streaming"] = await processed(client) - before

    with_client(stop_and_wait)
    with_client(streaming)
    assert counts["streaming"] - counts["send"] == TRAILING


def test_send_streaming_resolves_in_two_stages() -> None:
    async def body(client: autd3.Client) -> None:
        done = await client.send_streaming(sine())
        assert isinstance(done, autd3.StreamFuture)
        assert await result_of(done) is None
        with pytest.raises(ValueError):
            await done

    with_client(body)


def test_send_streaming_reports_a_rejection_only_from_the_inner_await() -> None:
    async def body(client: autd3.Client) -> None:
        done = await client.send_streaming(rejected_in_the_middle())
        with pytest.raises(autd3_core.Autd3Error):
            await done

    with_client(body)


def test_encoded_frames_go_out_one_by_one_through_send_frame() -> None:
    async def body(client: autd3.Client) -> None:
        frames = autd3.Frames.encode(client.geometry(), sine())
        assert len(frames) > 1
        for frame in frames:
            assert isinstance(frame, autd3.Frame)
            future = await client.send_frame(frame)
            assert isinstance(future, autd3.ResponseFuture)
            response = await future
            assert len(response.status) == DEVICES
            response.check()

        future = await client.send_frame(autd3.Frames.encode(client.geometry(), rejected_silencer())[0])
        response = await future
        assert any(status != 0 for status in response.status)
        with pytest.raises(autd3_core.Autd3Error):
            response.check()

    with_client(body)


def test_an_encode_error_fails_send_before_anything_goes_out() -> None:
    invalid = autd3.commands.SetCpuConfig(autd3.commands.CpuConfig(failsafe_timeout=autd3.Duration.from_nanos(0)))

    async def body(client: autd3.Client) -> None:
        before = await processed(client)
        with pytest.raises(autd3_core.Autd3Error, match="failsafe_timeout"):
            await client.send([autd3.commands.Nop(), invalid])
        with pytest.raises(autd3_core.Autd3Error, match="failsafe_timeout"):
            await client.send_streaming([autd3.commands.Nop(), invalid])
        assert await processed(client) - before == 1

    with_client(body)


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


def test_each_is_sent_like_any_other_command() -> None:
    async def body(client: autd3.Client) -> None:
        command = each(lambda device: autd3.commands.ForceFan(device.idx() == 0))
        assert await result_of(client.send(command)) is None
        done = await client.send_streaming(command)
        await done
        with pytest.raises(autd3_core.Autd3Error):
            await client.send(each(lambda device: [sine(), rejected_silencer()] if device.idx() == 1 else None))

    with_client(body)


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


def test_the_order_inside_a_sequence_reaches_the_device() -> None:
    async def body(client: autd3.Client) -> None:
        assert await result_of(client.send((sine(), autd3.commands.SetSilencer()))) is None
        with pytest.raises(autd3_core.Autd3Error):
            await client.send((sine(), rejected_silencer()))
        assert await result_of(client.send(())) is None
        done = await client.send_streaming([])
        assert await result_of(done) is None

    with_client(body)


def test_a_non_command_is_rejected_when_send_is_called() -> None:
    async def body(client: autd3.Client) -> None:
        with pytest.raises(TypeError):
            client.send(loose(1))
        with pytest.raises(TypeError):
            client.send_streaming(loose("nop"))
        with pytest.raises(TypeError):
            client.send_frame(loose(autd3.commands.Nop()))

    with_client(body)
