from autd3_link_twincat import TwinCATLinkOption

addr = "169.254.0.1"
ams_net_id = "169.254.0.1.1.1"
# ANCHOR: api
TwinCATLinkOption.local()

TwinCATLinkOption.remote(addr, ams_net_id)
# ANCHOR_END: api
