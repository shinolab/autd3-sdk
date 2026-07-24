import asyncio

import autd3_link_nop as nop
from autd3 import Client, ClientConfig, MAX_INFLIGHT
from autd3.geometry import Autd3, Geometry


async def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

    link = nop.Nop()
    option = (
        # ANCHOR: config
        ClientConfig(
            timeout_cycles=10,
            max_inflight=MAX_INFLIGHT,
            max_resync_rounds=8,
            low_latency=False,
            reset_resend_cycles=2,
            rt_priority=None,
            rt_affinity=None,
            validate_state=True,
        )
        # ANCHOR_END: config
    )
    # ANCHOR: api
    await Client.open(geometry, link, option)
    # ANCHOR_END: api


asyncio.run(main())
