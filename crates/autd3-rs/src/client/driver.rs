use std::sync::Arc;
use std::time::Instant;

use crate::error::{Error, NetworkCause};

use super::session::Session;

pub struct Driver {
    session: Arc<Session>,
    is_shut_down: bool,
}

impl Driver {
    pub(crate) fn new(session: Arc<Session>) -> Self {
        Self {
            session,
            is_shut_down: false,
        }
    }

    pub fn run(mut self) -> Result<(), Error> {
        while let Some(deadline) = self.poll()? {
            self.wait(deadline)?;
        }
        Ok(())
    }

    pub fn poll(&mut self) -> Result<Option<Instant>, Error> {
        if self.is_shut_down {
            return self.shutdown_result().map(|()| None);
        }
        if self.session.is_stopping() {
            self.shut_down(Error::Closed);
            return Ok(None);
        }
        self.session
            .step()
            .map(Some)
            .map_err(|cause| self.fail(cause))
    }

    pub fn wait(&mut self, deadline: Instant) -> Result<(), Error> {
        if self.is_shut_down {
            return self.shutdown_result();
        }
        self.session.wait_readable(deadline).map_err(|e| {
            tracing::error!("waiting for a reply failed: {e}");
            self.fail(NetworkCause::new(e))
        })
    }

    fn shutdown_result(&self) -> Result<(), Error> {
        match self.session.closed_error() {
            Some(error @ Error::Network(_)) => Err(error),
            _ => Ok(()),
        }
    }

    fn fail(&mut self, cause: NetworkCause) -> Error {
        let error = Error::Network(cause);
        self.shut_down(error.clone());
        error
    }

    fn shut_down(&mut self, error: Error) {
        self.is_shut_down = true;
        self.session.close(error);
        tracing::debug!("driver stopped");
    }
}

impl Drop for Driver {
    fn drop(&mut self) {
        if !self.is_shut_down {
            self.shut_down(Error::Closed);
        }
    }
}

#[cfg(unix)]
impl std::os::fd::AsFd for Driver {
    fn as_fd(&self) -> std::os::fd::BorrowedFd<'_> {
        std::os::fd::AsFd::as_fd(self.session.socket())
    }
}

#[cfg(windows)]
impl std::os::windows::io::AsSocket for Driver {
    fn as_socket(&self) -> std::os::windows::io::BorrowedSocket<'_> {
        std::os::windows::io::AsSocket::as_socket(self.session.socket())
    }
}
