use std::io::{self, Write};
use std::net::{Ipv6Addr, Shutdown, SocketAddr, TcpStream};
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use autd3_rs_core::{BusStats, DeviceClock};

use super::message::{LINK_VERSION, Notice, Reader, Request, encode_send};
use super::{LinkError, ServerLink};
use crate::udp::state::{SharedState, StateChecker};
use crate::udp::wait_writable;

fn is_timeout(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted
    )
}

fn write_whole(
    mut dst: impl Write,
    mut buf: &[u8],
    mut wait: impl FnMut() -> io::Result<()>,
) -> io::Result<()> {
    while !buf.is_empty() {
        match dst.write(buf) {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(n) => buf = &buf[n..],
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => wait()?,
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

fn send_whole(stream: &TcpStream, buf: &[u8]) -> io::Result<()> {
    write_whole(stream, buf, || wait_writable(stream, Duration::MAX))
}

struct Writer {
    stream: TcpStream,
    buf: Vec<u8>,
}

pub(crate) struct ServerConn {
    writer: Mutex<Writer>,
    closer: TcpStream,
    msg_id: AtomicU16,
    state: Arc<SharedState>,
    device_clock: DeviceClock,
    stats: BusStats,
}

pub(crate) struct ServerReader {
    stream: TcpStream,
    reader: Reader,
    nonblocking: bool,
}

pub(crate) fn connect(
    link: &ServerLink,
    num_devices: usize,
) -> Result<(ServerConn, ServerReader), LinkError> {
    if link.timeout.is_zero() {
        return Err(LinkError::ZeroTimeout);
    }
    let addr = SocketAddr::from((Ipv6Addr::LOCALHOST, link.port));
    let deadline = Instant::now() + link.timeout;
    let mut stream = TcpStream::connect_timeout(&addr, link.timeout).map_err(|e| {
        tracing::debug!(%addr, "connecting to the server failed: {e}");
        LinkError::ServerNotFound(addr)
    })?;
    stream.set_nodelay(true)?;
    let mut hello = Vec::new();
    Request::Hello {
        version: LINK_VERSION,
        num_devices: u16::try_from(num_devices).unwrap_or(u16::MAX),
    }
    .encode(&mut hello);
    stream.write_all(&hello)?;
    let mut reader = Reader::new();
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(LinkError::Timeout(link.timeout));
        }
        stream.set_read_timeout(Some(remaining))?;
        match reader.fill(&mut stream) {
            Ok(0) => return Err(LinkError::Disconnected),
            Ok(_) => {}
            Err(e) if is_timeout(&e) => continue,
            Err(e) => return Err(e.into()),
        }
        let Some((kind, body)) = reader.next_message()? else {
            continue;
        };
        match Notice::decode(kind, body)? {
            Notice::Welcome {
                version,
                num_devices: announced,
            } => {
                if version != LINK_VERSION {
                    return Err(LinkError::UnsupportedVersion {
                        server: version,
                        client: LINK_VERSION,
                    });
                }
                if usize::from(announced) != num_devices {
                    return Err(LinkError::DeviceCountMismatch {
                        server: usize::from(announced),
                        client: num_devices,
                    });
                }
                break;
            }
            Notice::Reject { version, .. } if version != LINK_VERSION => {
                return Err(LinkError::UnsupportedVersion {
                    server: version,
                    client: LINK_VERSION,
                });
            }
            Notice::Reject { reason, .. } => return Err(LinkError::Rejected(reason.to_owned())),
            _ => return Err(LinkError::Unexpected("a message before the welcome")),
        }
    }
    tracing::info!(%addr, num_devices, "connected to the server");
    stream.set_read_timeout(None)?;
    let conn = ServerConn {
        writer: Mutex::new(Writer {
            stream: stream.try_clone()?,
            buf: Vec::new(),
        }),
        closer: stream.try_clone()?,
        msg_id: AtomicU16::new(0),
        state: SharedState::new(num_devices),
        device_clock: DeviceClock::new(),
        stats: BusStats::default(),
    };
    let reader = ServerReader {
        stream,
        reader,
        nonblocking: false,
    };
    Ok((conn, reader))
}

impl ServerConn {
    fn writer(&self) -> MutexGuard<'_, Writer> {
        self.writer.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn write(&self, encode: impl FnOnce(&mut Vec<u8>)) -> io::Result<()> {
        let mut writer = self.writer();
        let Writer { stream, buf } = &mut *writer;
        buf.clear();
        encode(buf);
        send_whole(stream, buf)
    }

    pub(crate) fn reserve_msg_id(&self) -> u16 {
        self.msg_id.fetch_add(1, Ordering::Relaxed).wrapping_add(1)
    }

    pub(crate) fn send<F: AsRef<[u8]>>(
        &self,
        msg_id: u16,
        broadcast: bool,
        frames: &[F],
    ) -> io::Result<()> {
        self.write(|buf| encode_send(buf, msg_id, broadcast, frames))
    }

    pub(crate) fn admit_recovered(&self) -> io::Result<()> {
        self.write(|buf| Request::Admit.encode(buf))
    }

    pub(crate) fn recover(&self) -> io::Result<()> {
        self.write(|buf| Request::Recover.encode(buf))
    }

    pub(crate) fn wake(&self) -> io::Result<()> {
        self.write(|buf| Request::Wake.encode(buf))
    }

    pub(crate) fn state(&self) -> &SharedState {
        &self.state
    }

    pub(crate) fn state_checker(&self) -> StateChecker {
        StateChecker::new(Arc::clone(&self.state))
    }

    pub(crate) fn device_clock(&self) -> DeviceClock {
        self.device_clock.clone()
    }

    pub(crate) fn stats(&self) -> BusStats {
        self.stats.clone()
    }

    pub(crate) fn close(&self) {
        let _ = self.closer.shutdown(Shutdown::Both);
        self.state.close();
    }
}

impl ServerReader {
    fn set_nonblocking(&mut self, nonblocking: bool) -> io::Result<()> {
        if self.nonblocking != nonblocking {
            self.stream.set_nonblocking(nonblocking)?;
            self.nonblocking = nonblocking;
        }
        Ok(())
    }

    pub(crate) fn drain(
        &mut self,
        mut handle: impl FnMut(Notice<'_>) -> Result<(), LinkError>,
    ) -> Result<(), LinkError> {
        self.set_nonblocking(true)?;
        loop {
            while let Some((kind, body)) = self.reader.next_message()? {
                handle(Notice::decode(kind, body)?)?;
            }
            match self.reader.fill(&mut self.stream) {
                Ok(0) => return Err(LinkError::Disconnected),
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) if is_timeout(&e) => return Ok(()),
                Err(e) => return Err(e.into()),
            }
        }
    }

    pub(crate) fn wait_readable(&mut self, deadline: Instant) -> io::Result<()> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok(());
        }
        self.set_nonblocking(false)?;
        self.stream.set_read_timeout(Some(remaining))?;
        match self.stream.peek(&mut [0]) {
            Ok(_) => Ok(()),
            Err(e) if is_timeout(&e) => Ok(()),
            Err(e) => Err(e),
        }
    }

    pub(crate) fn stream(&self) -> &TcpStream {
        &self.stream
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::io::Read;
    use std::net::TcpListener;

    use super::*;

    struct Choked<'a> {
        accepted: Vec<u8>,
        script: &'a [io::Result<usize>],
        calls: usize,
    }

    impl Write for Choked<'_> {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let step = self.script.get(self.calls);
            self.calls += 1;
            match step {
                Some(Ok(n)) => {
                    let n = (*n).min(buf.len());
                    self.accepted.extend_from_slice(&buf[..n]);
                    Ok(n)
                }
                Some(Err(e)) => Err(io::Error::from(e.kind())),
                None => {
                    self.accepted.extend_from_slice(buf);
                    Ok(buf.len())
                }
            }
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn would_block() -> io::Result<usize> {
        Err(io::ErrorKind::WouldBlock.into())
    }

    #[test]
    fn a_message_cut_by_a_full_socket_is_written_out_whole_once_it_drains() {
        let mut message = Vec::new();
        encode_send(&mut message, 7, false, &[[0xA5u8; 300], [0x5Au8; 300]]);
        let script = [
            would_block(),
            Ok(100),
            would_block(),
            would_block(),
            Ok(250),
            would_block(),
        ];
        let mut dst = Choked {
            accepted: Vec::new(),
            script: &script,
            calls: 0,
        };
        let waits = Cell::new(0);

        write_whole(&mut dst, &message, || {
            waits.set(waits.get() + 1);
            Ok(())
        })
        .unwrap();

        assert_eq!(dst.accepted, message);
        assert_eq!(waits.get(), 4);
    }

    #[test]
    fn an_interrupted_write_is_retried_without_waiting() {
        let script = [
            Err(io::ErrorKind::Interrupted.into()),
            Ok(2),
            Err(io::ErrorKind::Interrupted.into()),
        ];
        let mut dst = Choked {
            accepted: Vec::new(),
            script: &script,
            calls: 0,
        };

        write_whole(&mut dst, b"abcdef", || {
            panic!("an interrupted write does not wait for the socket")
        })
        .unwrap();

        assert_eq!(dst.accepted, b"abcdef");
    }

    #[test]
    fn a_failed_wait_stops_the_write() {
        let script = [Ok(1), would_block()];
        let mut dst = Choked {
            accepted: Vec::new(),
            script: &script,
            calls: 0,
        };

        let result = write_whole(&mut dst, b"abc", || {
            Err(io::ErrorKind::ConnectionReset.into())
        });

        assert_eq!(
            result.map_err(|e| e.kind()),
            Err(io::ErrorKind::ConnectionReset)
        );
        assert_eq!(dst.accepted, b"a");
    }

    #[test]
    fn a_socket_that_takes_nothing_fails_instead_of_spinning() {
        let script = [Ok(0)];
        let mut dst = Choked {
            accepted: Vec::new(),
            script: &script,
            calls: 0,
        };

        let result = write_whole(&mut dst, b"abc", || Ok(()));

        assert_eq!(result.map_err(|e| e.kind()), Err(io::ErrorKind::WriteZero));
    }

    #[test]
    fn a_message_behind_a_full_nonblocking_socket_reaches_the_peer_whole() {
        let listener = TcpListener::bind((Ipv6Addr::LOCALHOST, 0)).unwrap();
        let sender = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut peer, _) = listener.accept().unwrap();
        sender.set_nonblocking(true).unwrap();
        let chunk = vec![0xEEu8; 1 << 16];
        let mut filled = 0;
        loop {
            match (&sender).write(&chunk) {
                Ok(n) => filled += n,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => panic!("filling the socket failed: {e}"),
            }
        }
        let message: Vec<u8> = (0..1u32 << 18)
            .map(|i| i.to_le_bytes()[0] ^ i.to_le_bytes()[1])
            .collect();

        let (sent, received) = std::thread::scope(|scope| {
            let writer = scope.spawn(|| {
                let sent = send_whole(&sender, &message);
                sender.shutdown(Shutdown::Write).unwrap();
                sent
            });
            let mut received = Vec::new();
            peer.read_to_end(&mut received).unwrap();
            (writer.join().unwrap(), received)
        });

        sent.unwrap();
        assert_eq!(received.len(), filled + message.len());
        assert!(received[..filled].iter().all(|&b| b == 0xEE));
        assert_eq!(received[filled..], message);
    }
}
