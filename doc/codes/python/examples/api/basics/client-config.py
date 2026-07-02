import asyncio

import autd3
import autd3_link_nop as nop


async def main() -> None:
    geometry = autd3.geometry.Geometry([autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

    link = nop.Nop()
    option = (
        # ANCHOR: config
        autd3.ClientConfig(
            timeout_cycles=10,
            max_inflight=127,
            send_interval_cycles=1,
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
    await autd3.Client.open(geometry, link, option)
    # ANCHOR_END: api


asyncio.run(main())
