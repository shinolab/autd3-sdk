use std::io::{IsTerminal, Stderr, Stdout, Write};

use tracing_appender::non_blocking::WorkerGuard;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
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

impl Sink {
    fn is_terminal(&self) -> bool {
        match self {
            Self::Stdout(w) => w.is_terminal(),
            Self::Stderr(w) => w.is_terminal(),
        }
    }
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

#[derive(Debug, thiserror::Error)]
#[error("failed to install the tracing subscriber: {0}")]
pub struct TracingInitError(#[source] Box<dyn std::error::Error + Send + Sync>);

pub fn init_tracing(option: TracingOption) -> TracingGuard {
    try_init_tracing(option).expect("the tracing subscriber is installed once per process")
}

pub fn try_init_tracing(option: TracingOption) -> Result<TracingGuard, TracingInitError> {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(option.default_filter));
    let sink = match option.writer {
        LogWriter::Stdout => Sink::Stdout(std::io::stdout()),
        LogWriter::Stderr => Sink::Stderr(std::io::stderr()),
    };
    let no_color = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
    let ansi = sink.is_terminal() && !no_color;
    let (writer, guard) = tracing_appender::non_blocking(sink);
    let builder = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(writer)
        .with_ansi(ansi);
    let installed = if std::env::var_os("JOURNAL_STREAM").is_some() {
        builder.without_time().try_init()
    } else {
        builder.try_init()
    };
    installed.map_err(TracingInitError)?;
    Ok(TracingGuard(guard))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_initialization_is_an_error() {
        let option = TracingOption {
            default_filter: "off",
            writer: LogWriter::Stderr,
        };

        let _first = try_init_tracing(option);
        let second = try_init_tracing(option);

        assert!(
            second
                .err()
                .is_some_and(|e| e.to_string().contains("tracing subscriber"))
        );
    }
}
