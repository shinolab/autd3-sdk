use std::io::{self, Read};

use autd3_rs_core::DeviceState;

pub const LINK_VERSION: u16 = 1;
pub const MESSAGE_BYTES_MAX: usize = 1 << 20;

const LEN_BYTES: usize = size_of::<u32>();
const READ_CHUNK: usize = 64 * 1024;

const HELLO: u8 = 0x01;
const SEND: u8 = 0x02;
const ADMIT: u8 = 0x03;
const WAKE_REQUEST: u8 = 0x04;
const WELCOME: u8 = 0x81;
const REJECT: u8 = 0x82;
const SETTLED: u8 = 0x83;
const REPLY: u8 = 0x84;
const STATES: u8 = 0x85;
const RECOVERED: u8 = 0x86;
const CLOCK: u8 = 0x87;
const WAKE: u8 = 0x88;
const FAILURE: u8 = 0x89;

const OUTCOME_SENT: u8 = 0;
const OUTCOME_UNSENT: u8 = 1;
const OUTCOME_FAILED: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MessageError {
    #[error("message kind {0:#04x} is unknown")]
    UnknownKind(u8),
    #[error("a message of kind {0:#04x} is truncated")]
    Truncated(u8),
    #[error("a message of {0} bytes exceeds the limit")]
    TooLong(usize),
    #[error("a message carries text that is not UTF-8")]
    NotUtf8,
    #[error("device state {0} is unknown")]
    UnknownState(u8),
    #[error("outcome {0} is unknown")]
    UnknownOutcome(u8),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request<'a> {
    Hello {
        version: u16,
        num_devices: u16,
    },
    Send {
        msg_id: u16,
        broadcast: bool,
        frames: Vec<&'a [u8]>,
    },
    Admit,
    Wake,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome<'a> {
    Sent {
        devices: u128,
    },
    Unsent {
        devices: u128,
        device: u16,
        reason: &'a str,
    },
    Failed {
        reason: &'a str,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice<'a> {
    Welcome {
        version: u16,
        num_devices: u16,
    },
    Reject {
        version: u16,
        reason: &'a str,
    },
    Settled {
        msg_id: u16,
        outcome: Outcome<'a>,
    },
    Reply {
        msg_id: u16,
        device: u8,
        ack: u8,
        status: u8,
        data: &'a [u8],
    },
    States(Vec<DeviceState>),
    Recovered(u128),
    Clock(u64),
    Wake,
    Failure(&'a str),
}

fn begin(out: &mut Vec<u8>, kind: u8) -> usize {
    let start = out.len();
    out.extend_from_slice(&[0; LEN_BYTES]);
    out.push(kind);
    start
}

fn finish(out: &mut [u8], start: usize) {
    let len = u32::try_from(out.len() - start - LEN_BYTES).unwrap_or(u32::MAX);
    out[start..start + LEN_BYTES].copy_from_slice(&len.to_le_bytes());
}

fn put_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

const fn state_code(state: DeviceState) -> u8 {
    match state {
        DeviceState::Ready => 0,
        DeviceState::Syncing => 1,
        DeviceState::Lost => 2,
        _ => unreachable!(),
    }
}

const fn state_from_code(code: u8) -> Result<DeviceState, MessageError> {
    match code {
        0 => Ok(DeviceState::Ready),
        1 => Ok(DeviceState::Syncing),
        2 => Ok(DeviceState::Lost),
        code => Err(MessageError::UnknownState(code)),
    }
}

pub fn encode_send<F: AsRef<[u8]>>(out: &mut Vec<u8>, msg_id: u16, broadcast: bool, frames: &[F]) {
    let start = begin(out, SEND);
    put_u16(out, msg_id);
    out.push(u8::from(broadcast));
    for frame in frames {
        let frame = frame.as_ref();
        put_u16(out, u16::try_from(frame.len()).unwrap_or(u16::MAX));
        out.extend_from_slice(frame);
    }
    finish(out, start);
}

struct Cursor<'a> {
    kind: u8,
    body: &'a [u8],
}

impl<'a> Cursor<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], MessageError> {
        if self.body.len() < len {
            return Err(MessageError::Truncated(self.kind));
        }
        let (head, rest) = self.body.split_at(len);
        self.body = rest;
        Ok(head)
    }

    fn u8(&mut self) -> Result<u8, MessageError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, MessageError> {
        let bytes = self.take(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn u64(&mut self) -> Result<u64, MessageError> {
        let mut bytes = [0; 8];
        bytes.copy_from_slice(self.take(8)?);
        Ok(u64::from_le_bytes(bytes))
    }

    fn u128(&mut self) -> Result<u128, MessageError> {
        let mut bytes = [0; 16];
        bytes.copy_from_slice(self.take(16)?);
        Ok(u128::from_le_bytes(bytes))
    }

    fn rest(&mut self) -> &'a [u8] {
        std::mem::take(&mut self.body)
    }

    fn text(&mut self) -> Result<&'a str, MessageError> {
        std::str::from_utf8(self.rest()).map_err(|_| MessageError::NotUtf8)
    }
}

impl<'a> Request<'a> {
    pub fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Request::Hello {
                version,
                num_devices,
            } => {
                let start = begin(out, HELLO);
                put_u16(out, *version);
                put_u16(out, *num_devices);
                finish(out, start);
            }
            Request::Send {
                msg_id,
                broadcast,
                frames,
            } => encode_send(out, *msg_id, *broadcast, frames),
            Request::Admit => {
                let start = begin(out, ADMIT);
                finish(out, start);
            }
            Request::Wake => {
                let start = begin(out, WAKE_REQUEST);
                finish(out, start);
            }
        }
    }

    pub fn decode(kind: u8, body: &'a [u8]) -> Result<Self, MessageError> {
        let mut cursor = Cursor { kind, body };
        match kind {
            HELLO => Ok(Request::Hello {
                version: cursor.u16()?,
                num_devices: cursor.u16()?,
            }),
            SEND => {
                let msg_id = cursor.u16()?;
                let broadcast = cursor.u8()? != 0;
                let mut frames = Vec::new();
                while !cursor.body.is_empty() {
                    let len = usize::from(cursor.u16()?);
                    frames.push(cursor.take(len)?);
                }
                Ok(Request::Send {
                    msg_id,
                    broadcast,
                    frames,
                })
            }
            ADMIT => Ok(Request::Admit),
            WAKE_REQUEST => Ok(Request::Wake),
            kind => Err(MessageError::UnknownKind(kind)),
        }
    }
}

impl<'a> Notice<'a> {
    pub fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Notice::Welcome {
                version,
                num_devices,
            } => {
                let start = begin(out, WELCOME);
                put_u16(out, *version);
                put_u16(out, *num_devices);
                finish(out, start);
            }
            Notice::Reject { version, reason } => {
                let start = begin(out, REJECT);
                put_u16(out, *version);
                out.extend_from_slice(reason.as_bytes());
                finish(out, start);
            }
            Notice::Settled { msg_id, outcome } => {
                let start = begin(out, SETTLED);
                put_u16(out, *msg_id);
                let (code, devices, device, reason) = match outcome {
                    Outcome::Sent { devices } => (OUTCOME_SENT, *devices, 0, ""),
                    Outcome::Unsent {
                        devices,
                        device,
                        reason,
                    } => (OUTCOME_UNSENT, *devices, *device, *reason),
                    Outcome::Failed { reason } => (OUTCOME_FAILED, 0, 0, *reason),
                };
                out.push(code);
                out.extend_from_slice(&devices.to_le_bytes());
                put_u16(out, device);
                out.extend_from_slice(reason.as_bytes());
                finish(out, start);
            }
            Notice::Reply {
                msg_id,
                device,
                ack,
                status,
                data,
            } => {
                let start = begin(out, REPLY);
                put_u16(out, *msg_id);
                out.extend_from_slice(&[*device, *ack, *status]);
                out.extend_from_slice(data);
                finish(out, start);
            }
            Notice::States(states) => {
                let start = begin(out, STATES);
                out.extend(states.iter().map(|&state| state_code(state)));
                finish(out, start);
            }
            Notice::Recovered(devices) => {
                let start = begin(out, RECOVERED);
                out.extend_from_slice(&devices.to_le_bytes());
                finish(out, start);
            }
            Notice::Clock(sys_time) => {
                let start = begin(out, CLOCK);
                out.extend_from_slice(&sys_time.to_le_bytes());
                finish(out, start);
            }
            Notice::Wake => {
                let start = begin(out, WAKE);
                finish(out, start);
            }
            Notice::Failure(reason) => {
                let start = begin(out, FAILURE);
                out.extend_from_slice(reason.as_bytes());
                finish(out, start);
            }
        }
    }

    pub fn decode(kind: u8, body: &'a [u8]) -> Result<Self, MessageError> {
        let mut cursor = Cursor { kind, body };
        match kind {
            WELCOME => Ok(Notice::Welcome {
                version: cursor.u16()?,
                num_devices: cursor.u16()?,
            }),
            REJECT => Ok(Notice::Reject {
                version: cursor.u16()?,
                reason: cursor.text()?,
            }),
            SETTLED => {
                let msg_id = cursor.u16()?;
                let code = cursor.u8()?;
                let devices = cursor.u128()?;
                let device = cursor.u16()?;
                let reason = cursor.text()?;
                let outcome = match code {
                    OUTCOME_SENT => Outcome::Sent { devices },
                    OUTCOME_UNSENT => Outcome::Unsent {
                        devices,
                        device,
                        reason,
                    },
                    OUTCOME_FAILED => Outcome::Failed { reason },
                    code => return Err(MessageError::UnknownOutcome(code)),
                };
                Ok(Notice::Settled { msg_id, outcome })
            }
            REPLY => Ok(Notice::Reply {
                msg_id: cursor.u16()?,
                device: cursor.u8()?,
                ack: cursor.u8()?,
                status: cursor.u8()?,
                data: cursor.rest(),
            }),
            STATES => cursor
                .rest()
                .iter()
                .map(|&code| state_from_code(code))
                .collect::<Result<_, _>>()
                .map(Notice::States),
            RECOVERED => Ok(Notice::Recovered(cursor.u128()?)),
            CLOCK => Ok(Notice::Clock(cursor.u64()?)),
            WAKE => Ok(Notice::Wake),
            FAILURE => Ok(Notice::Failure(cursor.text()?)),
            kind => Err(MessageError::UnknownKind(kind)),
        }
    }
}

#[derive(Debug, Default)]
pub struct Reader {
    buf: Vec<u8>,
    start: usize,
    end: usize,
}

impl Reader {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn fill(&mut self, src: &mut impl Read) -> io::Result<usize> {
        if self.start == self.end {
            self.start = 0;
            self.end = 0;
        }
        if self.buf.len() - self.end < READ_CHUNK {
            self.buf.copy_within(self.start..self.end, 0);
            self.end -= self.start;
            self.start = 0;
            if self.buf.len() - self.end < READ_CHUNK {
                self.buf.resize(self.end + READ_CHUNK, 0);
            }
        }
        let read = src.read(&mut self.buf[self.end..])?;
        self.end += read;
        Ok(read)
    }

    pub fn next_message(&mut self) -> Result<Option<(u8, &[u8])>, MessageError> {
        let pending = &self.buf[self.start..self.end];
        let Some(len) = pending.first_chunk::<LEN_BYTES>() else {
            return Ok(None);
        };
        let len = usize::try_from(u32::from_le_bytes(*len)).unwrap_or(usize::MAX);
        if !(1..=MESSAGE_BYTES_MAX).contains(&len) {
            return Err(MessageError::TooLong(len));
        }
        if pending.len() < LEN_BYTES + len {
            return Ok(None);
        }
        let kind = pending[LEN_BYTES];
        let body = self.start + LEN_BYTES + 1..self.start + LEN_BYTES + len;
        self.start += LEN_BYTES + len;
        Ok(Some((kind, &self.buf[body])))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_round_trip_through_one_stream() {
        let sent = [
            Request::Hello {
                version: LINK_VERSION,
                num_devices: 3,
            },
            Request::Send {
                msg_id: 0xBEEF,
                broadcast: false,
                frames: vec![&[1, 2, 3], &[], &[4; 300]],
            },
            Request::Send {
                msg_id: 7,
                broadcast: true,
                frames: vec![&[9, 9]],
            },
            Request::Admit,
            Request::Wake,
        ];
        let mut bytes = Vec::new();
        for request in &sent {
            request.encode(&mut bytes);
        }
        let mut reader = Reader::new();
        reader.fill(&mut &bytes[..]).unwrap();
        for expected in &sent {
            let (kind, body) = reader.next_message().unwrap().unwrap();
            assert_eq!(&Request::decode(kind, body).unwrap(), expected);
        }
        assert_eq!(reader.next_message(), Ok(None));
    }

    #[test]
    fn notices_round_trip() {
        let sent = [
            Notice::Welcome {
                version: LINK_VERSION,
                num_devices: 2,
            },
            Notice::Reject {
                version: LINK_VERSION,
                reason: "busy",
            },
            Notice::Settled {
                msg_id: 3,
                outcome: Outcome::Sent { devices: 0b11 },
            },
            Notice::Settled {
                msg_id: 4,
                outcome: Outcome::Unsent {
                    devices: 0b01,
                    device: 1,
                    reason: "unreachable",
                },
            },
            Notice::Settled {
                msg_id: 5,
                outcome: Outcome::Failed { reason: "closed" },
            },
            Notice::Reply {
                msg_id: 6,
                device: 1,
                ack: 2,
                status: 0,
                data: &[1, 2, 3, 4],
            },
            Notice::States(vec![
                DeviceState::Ready,
                DeviceState::Syncing,
                DeviceState::Lost,
            ]),
            Notice::Recovered(1 << 127),
            Notice::Clock(u64::MAX - 1),
            Notice::Wake,
            Notice::Failure("the socket closed"),
        ];
        let mut bytes = Vec::new();
        for notice in &sent {
            notice.encode(&mut bytes);
        }
        let mut reader = Reader::new();
        reader.fill(&mut &bytes[..]).unwrap();
        for expected in &sent {
            let (kind, body) = reader.next_message().unwrap().unwrap();
            assert_eq!(&Notice::decode(kind, body).unwrap(), expected);
        }
        assert_eq!(reader.next_message(), Ok(None));
    }

    #[test]
    fn a_message_split_across_reads_is_returned_once_complete() {
        let mut bytes = Vec::new();
        Notice::Clock(42).encode(&mut bytes);
        let mut reader = Reader::new();
        reader.fill(&mut &bytes[..5]).unwrap();
        assert_eq!(reader.next_message(), Ok(None));
        reader.fill(&mut &bytes[5..]).unwrap();
        let (kind, body) = reader.next_message().unwrap().unwrap();
        assert_eq!(Notice::decode(kind, body), Ok(Notice::Clock(42)));
    }

    #[test]
    fn malformed_messages_are_errors() {
        assert_eq!(
            Request::decode(0x7F, &[]),
            Err(MessageError::UnknownKind(0x7F))
        );
        assert_eq!(
            Request::decode(SEND, &[1, 0, 0, 5, 0, 1]),
            Err(MessageError::Truncated(SEND))
        );
        assert_eq!(
            Notice::decode(STATES, &[3]),
            Err(MessageError::UnknownState(3))
        );
        assert_eq!(Notice::decode(FAILURE, &[0xFF]), Err(MessageError::NotUtf8));
        let mut reader = Reader::new();
        reader.fill(&mut &[0u8, 0, 0, 0][..]).unwrap();
        assert_eq!(reader.next_message(), Err(MessageError::TooLong(0)));
    }

    #[test]
    fn the_buffer_is_reused_after_the_messages_are_consumed() {
        let mut bytes = Vec::new();
        Notice::Wake.encode(&mut bytes);
        let mut reader = Reader::new();
        for _ in 0..3 {
            reader.fill(&mut &bytes[..]).unwrap();
            assert_eq!(
                reader.next_message().map(|next| next.map(|(kind, _)| kind)),
                Ok(Some(WAKE))
            );
        }
        assert_eq!(reader.buf.len(), READ_CHUNK);
    }
}
