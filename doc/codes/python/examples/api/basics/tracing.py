from autd3 import LogWriter, init_tracing

# ANCHOR: api
log_guard = init_tracing(
    default_filter="info",
    writer=LogWriter.Stdout,
)
# ANCHOR_END: api

log_guard.close()
