use autd3_rs::rt::{LogWriter, TracingOption, init_tracing};

fn main() {
    // ANCHOR: api
    let option = TracingOption {
        default_filter: "info",
        writer: LogWriter::Stdout,
    };
    let _log_guard = init_tracing(option);
    // ANCHOR_END: api
}
