use std::net::SocketAddr;

use ads::{AmsAddr, Client, Source, Timeouts};
use autd3_rs_core::{DeviceState, LinkStatus, RX_FRAME_BYTES};

use crate::error::TwinCATLinkError;
use crate::link::{AUTD_INDEX_GROUP, AUTD_INDEX_OFFSET_RX};

const STATE_BYTES_PER_DEVICE: usize = 2;

const AL_STATE_MASK: u8 = 0x0F;
const AL_STATE_SAFE_OP: u8 = 0x04;
const AL_STATE_OP: u8 = 0x08;
const AL_ERROR_FLAG: u8 = 0x10;

fn map_state_word(low: u8) -> DeviceState {
    let state = low & AL_STATE_MASK;
    let error = low & AL_ERROR_FLAG != 0;
    match (state, error) {
        (AL_STATE_OP, _) => DeviceState::Op,
        (AL_STATE_SAFE_OP, true) => DeviceState::SafeOpError,
        (AL_STATE_SAFE_OP, false) => DeviceState::SafeOp,
        (bits, _) => DeviceState::Other(bits),
    }
}

pub struct TwinCATStateChecker {
    conn_addr: SocketAddr,
    source: Source,
    timeouts: Timeouts,
    ams_addr: AmsAddr,
    state_offset: u32,
    num_devices: usize,
    client: Option<Client>,
    states: Vec<DeviceState>,
}

impl TwinCATStateChecker {
    pub(crate) fn new(
        conn_addr: SocketAddr,
        source: Source,
        timeouts: Timeouts,
        ams_addr: AmsAddr,
        num_devices: usize,
    ) -> Self {
        let state_offset = AUTD_INDEX_OFFSET_RX
            + u32::try_from(num_devices * RX_FRAME_BYTES).expect("input image size exceeds u32");
        Self {
            conn_addr,
            source,
            timeouts,
            ams_addr,
            state_offset,
            num_devices,
            client: None,
            states: vec![DeviceState::Op; num_devices],
        }
    }

    fn check(&mut self) -> Result<LinkStatus, TwinCATLinkError> {
        let client = match &self.client {
            Some(client) => client,
            None => self
                .client
                .insert(Client::new(self.conn_addr, self.timeouts, self.source)?),
        };

        let mut buf = vec![0u8; self.num_devices * STATE_BYTES_PER_DEVICE];
        match client
            .device(self.ams_addr)
            .read_exact(AUTD_INDEX_GROUP, self.state_offset, &mut buf)
        {
            Ok(()) => {
                for (device, state) in self.states.iter_mut().enumerate() {
                    let new_state = map_state_word(buf[device * STATE_BYTES_PER_DEVICE]);
                    if new_state != *state {
                        tracing::info!(device, state = %new_state, "device state changed");
                        *state = new_state;
                    }
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to read EtherCAT state; marking devices lost");
                self.client = None;
                self.states.iter_mut().for_each(|s| *s = DeviceState::Lost);
            }
        }

        Ok(LinkStatus {
            devices: self.states.clone(),
            recoveries: 0,
        })
    }
}

impl autd3_rs_core::StateCheck for TwinCATStateChecker {
    type Error = TwinCATLinkError;

    fn check(&mut self) -> impl Future<Output = Result<LinkStatus, Self::Error>> + Send {
        std::future::ready(TwinCATStateChecker::check(self))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_word_mapping() {
        assert_eq!(DeviceState::Op, map_state_word(0x08));
        assert_eq!(DeviceState::SafeOp, map_state_word(0x04));
        assert_eq!(DeviceState::SafeOpError, map_state_word(0x14));
        assert_eq!(DeviceState::Other(0x01), map_state_word(0x01));
        assert_eq!(DeviceState::Other(0x02), map_state_word(0x02));
        assert_eq!(DeviceState::Other(0x03), map_state_word(0x03));
    }
}
