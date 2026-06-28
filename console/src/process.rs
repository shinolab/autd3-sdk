use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};

pub struct ManagedProcess {
    child: Child,
    rx: Receiver<String>,
    logs: Vec<String>,
    exited: bool,
}

impl ManagedProcess {
    pub fn spawn(program: &Path, args: &[String]) -> std::io::Result<Self> {
        let mut child = Command::new(program)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
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
            logs: Vec::new(),
            exited: false,
        })
    }

    pub fn pump(&mut self) {
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
            self.logs.push(exit_marker(status));
            self.exited = true;
        }
    }

    pub fn is_running(&self) -> bool {
        !self.exited
    }

    pub fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    pub fn logs(&self) -> &[String] {
        &self.logs
    }
}

impl Drop for ManagedProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn exit_marker(status: ExitStatus) -> String {
    match status.code() {
        Some(0) => "[process finished]".to_string(),
        Some(code) => format!("[process exited with code {code}]"),
        None => "[process terminated]".to_string(),
    }
}

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
