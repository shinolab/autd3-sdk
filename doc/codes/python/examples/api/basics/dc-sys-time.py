from autd3 import Duration
from autd3.value import DcSysTime

# ANCHOR: construct
epoch = DcSysTime.ZERO
now = DcSysTime.now()
raw = DcSysTime.from_nanos(1_000_000_000)
ns = now.sys_time
# ANCHOR_END: construct

# ANCHOR: ops
future = DcSysTime.now() + Duration.from_millis(100)
past = future - Duration.from_millis(50)
# ANCHOR_END: ops

_ = (epoch, raw, ns, past)
