from datetime import timedelta

from autd3_link_remote import RemoteLinkOption

addr = "127.0.0.1:8080"
# ANCHOR: api
RemoteLinkOption(addr)
# ANCHOR_END: api


def discover() -> RemoteLinkOption:
    # ANCHOR: discover
    option = RemoteLinkOption(RemoteLinkOption.discover())
    # ANCHOR_END: discover
    return option


def discover_with_option() -> RemoteLinkOption:
    # ANCHOR: discover_option
    option = RemoteLinkOption(
        RemoteLinkOption.discover(timeout=timedelta(seconds=5), instance="autd3-0a1b2c3d")
    )
    # ANCHOR_END: discover_option
    return option
