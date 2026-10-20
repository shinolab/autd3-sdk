use tracing_subscriber::EnvFilter;

fn main() {
    // ANCHOR: api
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
    // ANCHOR_END: api
}
