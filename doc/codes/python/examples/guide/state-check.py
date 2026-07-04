import asyncio

import autd3_link_ethercrab as ethercrab
from autd3 import Client, ClientConfig
from autd3.geometry import Autd3, Geometry

# xtask:long-running  # [hide]

CHECK_INTERVAL = 0.1


async def main() -> None:
    geometry = Geometry([Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

    # ANCHOR: open
    client, checker = await Client.open_with_checker(
        geometry,
        ethercrab.EtherCrabLinkOption(),
        ClientConfig(),
    )
    # ANCHOR_END: open

    try:
        # ANCHOR: poll
        last = None
        while True:
            status = await checker.check()
            if status != last:
                for i, state in enumerate(status.device_states):
                    print(f"device[{i}]: {state}")
                print(f"all operational: {status.all_op}, any lost: {status.any_lost}, recoveries: {status.recoveries}")
                last = status
            await asyncio.sleep(CHECK_INTERVAL)
        # ANCHOR_END: poll
    finally:
        await client.close()


if __name__ == "__main__":
    asyncio.run(main())
