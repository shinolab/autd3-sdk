import asyncio

from autd3 import Client, DeviceStatus, Duration, TransportOption
from autd3.commands import CpuConfig, ReleaseFailsafe, SetCpuConfig, each
from autd3.geometry import Autd3, Geometry

async def main() -> None:
    geometry = Geometry([Autd3()])

    # ANCHOR: heartbeat
    client = Client.open(geometry, TransportOption(heartbeat_interval=Duration.from_millis(10)))

    await client.send(SetCpuConfig(CpuConfig(failsafe_timeout=Duration.from_millis(100))))
    # ANCHOR_END: heartbeat

    with client:
        # ANCHOR: ptp_failsafe
        status = (await client.read_state()).device_status()

        def releasable(s: DeviceStatus) -> bool:
            return s.failsafe_active and s.ptp_locked

        if any(releasable(s) for s in status):
            await client.send(each(lambda device: ReleaseFailsafe() if releasable(status[device.idx()]) else None))
        # ANCHOR_END: ptp_failsafe

if __name__ == "__main__":
    asyncio.run(main())
