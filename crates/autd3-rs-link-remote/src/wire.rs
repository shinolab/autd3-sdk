use std::io::Read;

use autd3_rs_core::DeviceState;

use crate::error::PeerVersion;
use crate::{DeviceLayout, TransducerLayout};

pub(crate) const MAGIC: [u8; 4] = *b"ARMT";
pub(crate) const VERSION: u8 = 7;

pub(crate) const TAG_FRAME: u8 = 0x01;
pub(crate) const TAG_CLOSE: u8 = 0x02;

pub(crate) const SESSION_OK: u8 = 0x00;
pub(crate) const SESSION_BUS_CLOSED: u8 = 0x01;
pub(crate) const SESSION_DEVICE_COUNT: u8 = 0x02;
pub(crate) const SESSION_BUS_UNAVAILABLE: u8 = 0x03;
pub(crate) const SESSION_INTERNAL: u8 = 0x04;

pub(crate) const DC_TIME_BYTES: usize = 8;
pub(crate) const DC_TIME_UNAVAILABLE: u64 = 0;

pub(crate) const REPLY_HEADER_BYTES: usize = 1 + DC_TIME_BYTES + 1 + 5 * 8;

const DEV_STATE_SAFE_OP: u8 = 0x04;
const DEV_STATE_OP: u8 = 0x08;
const DEV_STATE_SAFE_OP_ERROR: u8 = 0x14;
const DEV_STATE_LOST: u8 = 0xff;
const DEV_STATE_OTHER_TAG: u8 = 0x20;
const DEV_STATE_OTHER_MASK: u8 = 0x1f;

const LAYOUT_BYTES: usize = size_of::<TransducerLayout>();

pub(crate) const MAX_DEVICES: usize = 1024;
pub(crate) const MAX_TRANSDUCERS: usize = 4096;

pub(crate) const SDK_VERSION: &str = env!("CARGO_PKG_VERSION");

#[must_use]
pub(crate) fn local_version() -> PeerVersion {
    PeerVersion {
        wire: VERSION,
        sdk: SDK_VERSION.to_owned(),
    }
}

pub(crate) fn encode_hello() -> Vec<u8> {
    let sdk = SDK_VERSION.as_bytes();
    let len = u8::try_from(sdk.len()).unwrap_or(0);
    let mut out = Vec::with_capacity(6 + usize::from(len));
    out.extend_from_slice(&MAGIC);
    out.push(VERSION);
    out.push(len);
    out.extend_from_slice(&sdk[..usize::from(len)]);
    out
}

pub(crate) fn read_hello(stream: &mut impl Read) -> std::io::Result<Option<PeerVersion>> {
    let mut head = [0u8; 6];
    stream.read_exact(&mut head)?;
    if head[..4] != MAGIC {
        return Ok(None);
    }
    let mut sdk = vec![0u8; usize::from(head[5])];
    stream.read_exact(&mut sdk)?;
    Ok(Some(PeerVersion {
        wire: head[4],
        sdk: String::from_utf8_lossy(&sdk).into_owned(),
    }))
}

#[must_use]
pub(crate) fn encode_session_ok(num_devices: u16) -> Vec<u8> {
    let mut out = Vec::with_capacity(3);
    out.push(SESSION_OK);
    out.extend_from_slice(&num_devices.to_le_bytes());
    out
}

#[must_use]
pub(crate) fn encode_session_reject(code: u8, detail: &str) -> Vec<u8> {
    let detail = detail.as_bytes();
    let len = u16::try_from(detail.len()).unwrap_or(u16::MAX);
    let mut out = Vec::with_capacity(3 + usize::from(len));
    out.push(code);
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&detail[..usize::from(len)]);
    out
}

pub(crate) fn read_session_reply(
    stream: &mut impl Read,
) -> std::io::Result<Result<usize, (u8, String)>> {
    let mut head = [0u8; 3];
    stream.read_exact(&mut head)?;
    let payload = u16::from_le_bytes([head[1], head[2]]);
    if head[0] == SESSION_OK {
        return Ok(Ok(usize::from(payload)));
    }
    let mut detail = vec![0u8; usize::from(payload)];
    stream.read_exact(&mut detail)?;
    Ok(Err((
        head[0],
        String::from_utf8_lossy(&detail).into_owned(),
    )))
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct BusStatus {
    pub(crate) recovering: bool,
    pub(crate) recoveries: u64,
    pub(crate) stale_cycles: u64,
    pub(crate) lost_cycles: u64,
    pub(crate) phase_excursions: u64,
    pub(crate) worst_phase_deviation_ns: u64,
    pub(crate) devices: Vec<DeviceState>,
}

impl BusStatus {
    pub(crate) fn new(num_devices: usize) -> Self {
        Self {
            devices: vec![DeviceState::Op; num_devices],
            ..Self::default()
        }
    }

    fn counters(&self) -> [u64; 5] {
        [
            self.recoveries,
            self.stale_cycles,
            self.lost_cycles,
            self.phase_excursions,
            self.worst_phase_deviation_ns,
        ]
    }
}

#[must_use]
pub(crate) fn encode_device_state(state: DeviceState) -> u8 {
    match state {
        DeviceState::Op => DEV_STATE_OP,
        DeviceState::SafeOp => DEV_STATE_SAFE_OP,
        DeviceState::SafeOpError => DEV_STATE_SAFE_OP_ERROR,
        DeviceState::Lost => DEV_STATE_LOST,
        DeviceState::Other(bits) => DEV_STATE_OTHER_TAG | (bits & DEV_STATE_OTHER_MASK),
    }
}

#[must_use]
pub(crate) fn decode_device_state(byte: u8) -> DeviceState {
    match byte {
        DEV_STATE_OP => DeviceState::Op,
        DEV_STATE_SAFE_OP => DeviceState::SafeOp,
        DEV_STATE_SAFE_OP_ERROR => DeviceState::SafeOpError,
        DEV_STATE_LOST => DeviceState::Lost,
        bits => DeviceState::Other(bits & DEV_STATE_OTHER_MASK),
    }
}

pub(crate) fn encode_reply_header(
    rx_valid: bool,
    dc_time_ns: u64,
    status: &BusStatus,
    out: &mut Vec<u8>,
) {
    out.clear();
    out.push(u8::from(rx_valid));
    out.extend_from_slice(&dc_time_ns.to_le_bytes());
    out.push(u8::from(status.recovering));
    out.extend(status.counters().into_iter().flat_map(u64::to_le_bytes));
    out.extend(status.devices.iter().copied().map(encode_device_state));
}

pub(crate) fn decode_reply_header(buf: &[u8], status: &mut BusStatus) -> (bool, u64) {
    let rx_valid = buf[0] != 0;
    let mut dc = [0u8; DC_TIME_BYTES];
    dc.copy_from_slice(&buf[1..=DC_TIME_BYTES]);
    status.recovering = buf[1 + DC_TIME_BYTES] != 0;

    let counters = &buf[2 + DC_TIME_BYTES..REPLY_HEADER_BYTES];
    let at = |i: usize| {
        let mut b = [0u8; 8];
        b.copy_from_slice(&counters[i * 8..i * 8 + 8]);
        u64::from_le_bytes(b)
    };
    status.recoveries = at(0);
    status.stale_cycles = at(1);
    status.lost_cycles = at(2);
    status.phase_excursions = at(3);
    status.worst_phase_deviation_ns = at(4);

    status.devices.clear();
    status.devices.extend(
        buf[REPLY_HEADER_BYTES..]
            .iter()
            .copied()
            .map(decode_device_state),
    );

    (rx_valid, u64::from_le_bytes(dc))
}

pub(crate) fn encode_geometry(devices: &[DeviceLayout]) -> Vec<u8> {
    let num_devices = u32::try_from(devices.len()).unwrap_or(u32::MAX);
    let total: usize = devices.iter().map(|d| d.transducers.len()).sum();
    let mut out = Vec::with_capacity(4 + devices.len() * 4 + total * LAYOUT_BYTES);
    out.extend_from_slice(&num_devices.to_le_bytes());
    out.extend(devices.iter().flat_map(|dev| {
        let n = u32::try_from(dev.transducers.len()).unwrap_or(u32::MAX);
        n.to_le_bytes()
            .into_iter()
            .chain(dev.transducers.iter().flat_map(|t| {
                t.pos
                    .iter()
                    .chain(t.dir.iter())
                    .flat_map(|v| v.to_le_bytes())
            }))
    }));
    out
}

fn too_large(what: &str, got: usize, max: usize) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!("{what} out of range: {got} exceeds the maximum of {max}"),
    )
}

pub(crate) fn read_geometry(stream: &mut impl Read) -> std::io::Result<Vec<DeviceLayout>> {
    let mut u32_buf = [0u8; 4];
    stream.read_exact(&mut u32_buf)?;
    let num_devices = u32::from_le_bytes(u32_buf) as usize;
    if num_devices > MAX_DEVICES {
        return Err(too_large("device count", num_devices, MAX_DEVICES));
    }
    let mut devices = Vec::with_capacity(num_devices);
    let mut buf = [0u8; LAYOUT_BYTES];
    for _ in 0..num_devices {
        stream.read_exact(&mut u32_buf)?;
        let n = u32::from_le_bytes(u32_buf) as usize;
        if n > MAX_TRANSDUCERS {
            return Err(too_large("transducer count", n, MAX_TRANSDUCERS));
        }
        let mut transducers = Vec::with_capacity(n);
        for _ in 0..n {
            stream.read_exact(&mut buf)?;
            let f = |i: usize| {
                let mut b = [0u8; 4];
                b.copy_from_slice(&buf[i * 4..i * 4 + 4]);
                f32::from_le_bytes(b)
            };
            transducers.push(TransducerLayout {
                pos: [f(0), f(1), f(2)],
                dir: [f(3), f(4), f(5)],
            });
        }
        devices.push(DeviceLayout { transducers });
    }
    Ok(devices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_states_survive_a_round_trip() {
        let named = [
            DeviceState::Op,
            DeviceState::SafeOp,
            DeviceState::SafeOpError,
            DeviceState::Lost,
        ];
        for state in named
            .into_iter()
            .chain((0..=DEV_STATE_OTHER_MASK).map(DeviceState::Other))
        {
            assert_eq!(decode_device_state(encode_device_state(state)), state);
        }
    }

    #[test]
    fn an_al_state_carrying_the_error_flag_never_decodes_as_a_healthy_one() {
        for bits in 0..=DEV_STATE_OTHER_MASK {
            let decoded = decode_device_state(encode_device_state(DeviceState::Other(bits)));
            assert_eq!(decoded, DeviceState::Other(bits));
            assert!(
                !matches!(
                    decoded,
                    DeviceState::Op | DeviceState::SafeOp | DeviceState::SafeOpError
                ),
                "{bits:#04x} decoded as {decoded}",
            );
        }
    }

    #[test]
    fn the_reply_header_carries_the_bus_status() {
        let status = BusStatus {
            recovering: true,
            recoveries: 3,
            stale_cycles: 17,
            lost_cycles: 5,
            phase_excursions: 41,
            worst_phase_deviation_ns: 987_654,
            devices: vec![DeviceState::Op, DeviceState::SafeOpError, DeviceState::Lost],
        };

        let mut buf = Vec::new();
        encode_reply_header(true, 1_234_567_890, &status, &mut buf);
        assert_eq!(buf.len(), REPLY_HEADER_BYTES + status.devices.len());

        let mut decoded = BusStatus::new(3);
        let (rx_valid, dc_time_ns) = decode_reply_header(&buf, &mut decoded);
        assert!(rx_valid);
        assert_eq!(dc_time_ns, 1_234_567_890);
        assert_eq!(decoded, status);
    }

    fn layout(num_devices: usize, num_transducers: usize) -> Vec<DeviceLayout> {
        vec![
            DeviceLayout {
                transducers: vec![
                    TransducerLayout {
                        pos: [1.0, 2.0, 3.0],
                        dir: [0.0, 0.0, 1.0],
                    };
                    num_transducers
                ],
            };
            num_devices
        ]
    }

    #[test]
    fn the_largest_accepted_geometry_survives_a_round_trip() {
        let devices = layout(MAX_DEVICES, 1);
        let encoded = encode_geometry(&devices);
        assert_eq!(read_geometry(&mut encoded.as_slice()).unwrap(), devices);

        let devices = layout(1, MAX_TRANSDUCERS);
        let encoded = encode_geometry(&devices);
        assert_eq!(read_geometry(&mut encoded.as_slice()).unwrap(), devices);
    }

    #[test]
    fn an_oversized_device_count_is_rejected_without_allocating() {
        for count in [u32::try_from(MAX_DEVICES).unwrap() + 1, u32::MAX] {
            let err = read_geometry(&mut count.to_le_bytes().as_slice()).unwrap_err();
            assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        }
    }

    #[test]
    fn an_oversized_transducer_count_is_rejected_without_allocating() {
        for count in [u32::try_from(MAX_TRANSDUCERS).unwrap() + 1, u32::MAX] {
            let mut frame = 1u32.to_le_bytes().to_vec();
            frame.extend_from_slice(&count.to_le_bytes());
            let err = read_geometry(&mut frame.as_slice()).unwrap_err();
            assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        }
    }
}
