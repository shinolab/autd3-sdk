import asyncio

from autd3 import Autd3Error, Client, DeviceStatus, Duration, TransportOption
from autd3.commands import CpuConfig, ReleaseFailsafe, SetCpuConfig, each
from autd3.geometry import Autd3, Geometry

async def main() -> None:
    geometry = Geometry([Autd3()])

    # ANCHOR: heartbeat
    client = Client.open(geometry, TransportOption())

    async def send_heartbeats() -> None:
        while True:
            try:
                client.send_heartbeat()
            except Autd3Error as e:
                print(f"sending a heartbeat failed: {e}")
            await asyncio.sleep(0.01)

    heartbeat = asyncio.create_task(send_heartbeats())

    await client.send(SetCpuConfig(CpuConfig(failsafe_timeout=Duration.from_millis(100))))
    # ANCHOR_END: heartbeat

    with client:
        # ANCHOR: ptp_failsafe
        msg_id = client.send_heartbeat()
        status = (await client.collect(msg_id)).device_status()

        def releasable(s: DeviceStatus) -> bool:
            return s.failsafe_active and s.ptp_locked

        if any(releasable(s) for s in status):
            await client.send(each(lambda device: ReleaseFailsafe() if releasable(status[device.idx()]) else None))
        # ANCHOR_END: ptp_failsafe

        heartbeat.cancel()

if __name__ == "__main__":
    asyncio.run(main())
