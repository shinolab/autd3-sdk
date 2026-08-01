use std::io::{Stderr, Stdout, Write};

use tracing_appender::non_blocking::WorkerGuard;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LogWriter {
    #[default]
    Stdout,
    Stderr,
}

#[derive(Clone, Copy, Debug)]
pub struct TracingOption {
    pub default_filter: &'static str,
    pub writer: LogWriter,
}

impl Default for TracingOption {
    fn default() -> Self {
        Self {
            default_filter: "info",
            writer: LogWriter::default(),
        }
    }
}

#[must_use]
pub struct TracingGuard(#[expect(dead_code)] WorkerGuard);

enum Sink {
    Stdout(Stdout),
    Stderr(Stderr),
}

impl Write for Sink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::Stdout(w) => w.write(buf),
            Self::Stderr(w) => w.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Stdout(w) => w.flush(),
            Self::Stderr(w) => w.flush(),
        }
    }
}

pub fn init_tracing(option: TracingOption) -> TracingGuard {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(option.default_filter));
    let sink = match option.writer {
        LogWriter::Stdout => Sink::Stdout(std::io::stdout()),
        LogWriter::Stderr => Sink::Stderr(std::io::stderr()),
    };
    let (writer, guard) = tracing_appender::non_blocking(sink);
    let builder = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(writer);
    if std::env::var_os("JOURNAL_STREAM").is_some() {
        builder.without_time().with_ansi(false).init();
    } else {
        builder.init();
    }
    TracingGuard(guard)
}
