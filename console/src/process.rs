use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};

pub struct ManagedProcess {
    child: Child,
    rx: Receiver<String>,
    logs: Vec<String>,
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
        })
    }

    pub fn pump(&mut self) {
        while let Ok(line) = self.rx.try_recv() {
            self.logs.push(line);
        }
    }

    pub fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
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
        self.kill();
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
