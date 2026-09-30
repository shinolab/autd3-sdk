import asyncio
import threading

from autd3 import Client, ClientConfig, Driver, Duration, TransportOption
from autd3.geometry import Autd3, Geometry


async def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

    udp = TransportOption()
    option = (
        # ANCHOR: config
        ClientConfig(
            ack_timeout=Duration.from_millis(10),
            max_inflight=7,
            max_resync_rounds=8,
            low_latency=False,
            validate_state=True,
            require_supported_firmware=False,
        )
        # ANCHOR_END: config
    )
    # ANCHOR: api
    driver, connector = Driver.open(udp, geometry.num_devices())
    threading.Thread(target=driver.run, daemon=True).start()
    await Client.open(geometry, connector, option)
    # ANCHOR_END: api


asyncio.run(main())
