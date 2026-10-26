use std::io::{BufRead, BufReader, Read};
use std::num::NonZeroUsize;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};

pub(crate) const FINISHED: &str = "[process finished]";

const LOG_CAPACITY: NonZeroUsize = NonZeroUsize::new(10_000).unwrap();

pub(crate) struct LogLines {
    lines: Vec<String>,
    capacity: NonZeroUsize,
}

impl LogLines {
    pub(crate) fn new(capacity: NonZeroUsize) -> Self {
        Self {
            lines: Vec::new(),
            capacity,
        }
    }

    pub(crate) fn push(&mut self, line: String) {
        let capacity = self.capacity.get();
        if self.lines.len() >= 2 * capacity {
            self.lines.drain(..self.lines.len() - capacity);
        }
        self.lines.push(line);
    }

    pub(crate) fn as_slice(&self) -> &[String] {
        &self.lines[self.lines.len().saturating_sub(self.capacity.get())..]
    }
}

impl Default for LogLines {
    fn default() -> Self {
        Self::new(LOG_CAPACITY)
    }
}

pub(crate) struct ManagedProcess {
    child: Child,
    rx: Receiver<String>,
    logs: LogLines,
    exited: bool,
}

impl ManagedProcess {
    pub(crate) fn spawn(program: &Path, args: &[String]) -> std::io::Result<Self> {
        let mut command = Command::new(program);
        command
            .args(args)
            .env("NO_COLOR", "1")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        no_window(&mut command);
        let mut child = command.spawn()?;
        let (tx, rx) = channel();
        if let Some(stdout) = child.stdout.take() {
            spawn_reader(stdout, tx.clone());
        }
        if let Some(stderr) = child.stderr.take() {
            spawn_reader(stderr, tx);
        }
        Ok(Self {
            child,
            rx,
            logs: LogLines::default(),
            exited: false,
        })
    }

    pub(crate) fn pump(&mut self) {
        if self.exited {
            return;
        }
        let mut disconnected = false;
        loop {
            match self.rx.try_recv() {
                Ok(line) => self.logs.push(line),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }
        if disconnected && let Ok(Some(status)) = self.child.try_wait() {
            let marker = match status.code() {
                Some(0) => FINISHED.to_string(),
                Some(code) => format!("[process exited with code {code}]"),
                None => "[process terminated]".to_string(),
            };
            self.logs.push(marker);
            self.exited = true;
        }
    }

    pub(crate) fn is_running(&self) -> bool {
        !self.exited
    }

    pub(crate) fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    pub(crate) fn logs(&self) -> &[String] {
        self.logs.as_slice()
    }
}

impl Drop for ManagedProcess {
    fn drop(&mut self) {
        self.kill();
    }
}

#[cfg(windows)]
pub(crate) fn no_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
pub(crate) fn no_window(_command: &mut Command) {}

fn spawn_reader<R: Read + Send + 'static>(reader: R, tx: Sender<String>) {
    std::thread::spawn(move || {
        for line in BufReader::new(reader).lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_log_never_holds_more_than_its_capacity() {
        let capacity = NonZeroUsize::new(3).unwrap();
        let mut logs = LogLines::new(capacity);
        assert_eq!(logs.as_slice(), [] as [String; 0]);
        for i in 0..100 {
            logs.push(i.to_string());
            assert!(logs.as_slice().len() <= capacity.get());
            assert!(logs.lines.len() <= 2 * capacity.get());
            assert_eq!(logs.as_slice().last(), Some(&i.to_string()));
        }
        assert_eq!(logs.as_slice(), ["97", "98", "99"]);
    }

    #[test]
    fn the_log_keeps_every_line_below_its_capacity() {
        let mut logs = LogLines::new(NonZeroUsize::new(4).unwrap());
        for line in ["a", "b", "c"] {
            logs.push(line.to_string());
        }
        assert_eq!(logs.as_slice(), ["a", "b", "c"]);
    }
}
