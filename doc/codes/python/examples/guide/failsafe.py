import asyncio

from autd3 import Client, DeviceStatus, Duration, TransportOption
from autd3.commands import CpuConfig, ReleaseFailsafe, SetCpuConfig, each
from autd3.geometry import Autd3, Geometry
# xtask:long-running  # [hide]

async def main() -> None:
    geometry = Geometry([Autd3()])

    client = Client.open(geometry, TransportOption())

    with client:
        # ANCHOR: ptp_failsafe
        status = client.device_status()

        def releasable(s: DeviceStatus) -> bool:
            return s.failsafe_active and s.ptp_locked

        if any(releasable(s) for s in status):
            await client.send(each(lambda device: ReleaseFailsafe() if releasable(status[device.idx()]) else None))
        # ANCHOR_END: ptp_failsafe

        # ANCHOR: heartbeat
        await client.send(SetCpuConfig(CpuConfig(failsafe_timeout=Duration.from_millis(100))))

        while True:
            client.send_heartbeat()
            await asyncio.sleep(0.01)
        # ANCHOR_END: heartbeat

if __name__ == "__main__":
    asyncio.run(main())
