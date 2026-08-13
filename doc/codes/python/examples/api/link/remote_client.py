from autd3 import Duration

from autd3_link_remote import RemoteLinkOption

addr = "127.0.0.1:8080"
timeout = Duration.from_secs(1)

# ANCHOR: api
RemoteLinkOption(addr, timeout)
# ANCHOR_END: api


def discover() -> None:
    # ANCHOR: discover
    RemoteLinkOption.discover()
    # ANCHOR_END: discover


def discover_with_option() -> None:
    # ANCHOR: discover_option
    RemoteLinkOption.discover(timeout, "autd3-0a1b2c3d")
    # ANCHOR_END: discover_option
