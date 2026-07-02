import asyncio

import autd3
import autd3_link_ethercrab as ethercrab

# xtask:long-running  # [hide]

CHECK_INTERVAL = 0.1


async def main() -> None:
    geometry = autd3.geometry.Geometry([autd3.geometry.Autd3([0.0, 0.0, 0.0], [1.0, 0.0, 0.0, 0.0])])

    # ANCHOR: open
    client, checker = await autd3.Client.open_with_checker(
        geometry,
        ethercrab.EtherCrabLinkOption(),
        autd3.ClientConfig(),
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
