import asyncio
import sys
import threading
import time

import pytest

import autd3
import autd3_core

DEVICES = 2


def geometry(n: int) -> autd3.geometry.Geometry:
    return autd3.geometry.Geometry(
        [autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0]) for _ in range(n)]
    )


async def stream(client: autd3.Client) -> None:
    builder = client.datagram_builder()
    builder.push(autd3.commands.SetSilencer())
    for frame in builder.build():
        await client.send_checked(frame)
    for _ in range(4):
        assert len(await client.read_firmware_version()) == client.num_devices()


def test_a_driver_runs_on_a_user_thread() -> None:
    emulator = autd3.UdpEmulator(DEVICES)
    driver, connector = autd3.Driver.open(emulator.option(), DEVICES)
    assert driver.num_devices == DEVICES
    assert connector.num_devices == DEVICES
    checker = driver.state_checker()
    runner = threading.Thread(target=driver.run, daemon=True)
    runner.start()

    async def run() -> None:
        async with await autd3.Client.open(geometry(DEVICES), connector, autd3.ClientConfig()) as client:
            with pytest.raises(autd3_core.Autd3Error):
                driver.poll()
            await stream(client)
            assert checker.check().all_ready

    asyncio.run(run())
    runner.join(timeout=5)
    assert not runner.is_alive()
    assert driver.is_closed()
    assert driver.poll() is None


def test_a_driver_polled_with_sleeps_still_serves_the_client() -> None:
    emulator = autd3.UdpEmulator(DEVICES)
    driver, connector = autd3.Driver.open(emulator.option(), DEVICES)
    checker = driver.state_checker()

    def drive() -> None:
        while driver.poll() is not None:
            time.sleep(0.02)

    runner = threading.Thread(target=drive, daemon=True)
    runner.start()

    async def run() -> None:
        async with await autd3.Client.open(geometry(DEVICES), connector, autd3.ClientConfig()) as client:
            await stream(client)
            assert checker.check().all_ready

    asyncio.run(run())
    runner.join(timeout=5)
    assert not runner.is_alive()


@pytest.mark.skipif(sys.platform == "win32", reason="the proactor event loop has no add_reader")
def test_a_driver_runs_on_the_asyncio_event_loop() -> None:
    emulator = autd3.UdpEmulator(DEVICES)
    driver, connector = autd3.Driver.open(emulator.option(), DEVICES)

    async def drive() -> None:
        loop = asyncio.get_running_loop()
        readable = asyncio.Event()
        fd = driver.fileno()
        loop.add_reader(fd, readable.set)
        notified: asyncio.Future[None] | None = None
        try:
            while (timeout := driver.poll()) is not None:
                readable.clear()
                if notified is None or notified.done():
                    notified = asyncio.ensure_future(driver.notified())
                socket = asyncio.ensure_future(readable.wait())
                await asyncio.wait({notified, socket}, timeout=timeout, return_when=asyncio.FIRST_COMPLETED)
                socket.cancel()
        finally:
            loop.remove_reader(fd)
            if notified is not None:
                notified.cancel()
        driver.close()

    async def run() -> None:
        task = asyncio.create_task(drive())
        async with await autd3.Client.open(geometry(DEVICES), connector, autd3.ClientConfig()) as client:
            await stream(client)
        await asyncio.wait_for(task, timeout=5)

    asyncio.run(run())
    assert driver.is_closed()


def test_a_closed_driver_resolves_its_waiters() -> None:
    emulator = autd3.UdpEmulator(1)
    driver, connector = autd3.Driver.open(emulator.option(), 1)
    del connector
    driver.run()
    assert driver.poll() is None
    driver.wait(0.01)

    async def run() -> None:
        await asyncio.wait_for(driver.notified(), timeout=5)

    asyncio.run(run())
    driver.close()


def test_a_connector_opens_only_once() -> None:
    emulator = autd3.UdpEmulator(1)
    driver, connector = autd3.Driver.open(emulator.option(), 1)
    runner = threading.Thread(target=driver.run, daemon=True)
    runner.start()

    async def run() -> None:
        async with await autd3.Client.open(geometry(1), connector, autd3.ClientConfig()):
            with pytest.raises(autd3_core.Autd3Error):
                await autd3.Client.open(geometry(1), connector, autd3.ClientConfig())

    asyncio.run(run())
    runner.join(timeout=5)
    assert not runner.is_alive()


def test_a_negative_wait_is_rejected() -> None:
    emulator = autd3.UdpEmulator(1)
    driver, _connector = autd3.Driver.open(emulator.option(), 1)
    with pytest.raises(ValueError):
        driver.wait(-1.0)
    driver.close()
