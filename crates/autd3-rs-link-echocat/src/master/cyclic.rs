use std::time::Duration;

use super::Master;
use super::init::{INPUT_BYTES, INPUT_LOGICAL_BASE, OUTPUT_BYTES, OUTPUT_LOGICAL_BASE};
use crate::bus::RawBus;
use crate::error::EchocatError;
use crate::reg;
use crate::wire::{
    Address, Command, DATAGRAM_OVERHEAD_BYTES, ECAT_HEADER_BYTES, ETH_HEADER_BYTES,
    FRAME_HEADER_BYTES, FrameBuilder, FrameView, MIN_ETHERNET_FRAME_BYTES, Slot,
};

pub const LOSE_CONTACT_AFTER_CYCLES: u32 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    DcTime,
    AlStatus,
    DeviceAlStatus,
    Inputs,
    Outputs { offset: usize },
}

#[derive(Clone, Copy, Debug)]
struct PlannedDatagram {
    command: Command,
    address: Address,
    len: usize,
    expected_wkc: u16,
    role: Role,
}

#[derive(Default)]
pub(crate) struct CyclePlan {
    frames: Vec<Vec<PlannedDatagram>>,
    buffers: Vec<Vec<u8>>,
    slots: Vec<Vec<Slot>>,
    indices: Vec<u8>,
    sent: Vec<usize>,
    echo_pending: Vec<bool>,
    received: Vec<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CycleReport {
    pub rx_valid: bool,
    pub dc_system_time: u64,
    pub al_status: u16,
}

#[must_use]
pub fn next_cycle_wait(dc_system_time: u64, cycle: Duration, landing_target_ns: u64) -> Duration {
    let cycle_ns = u64::try_from(cycle.as_nanos()).expect("cycle fits in u64 nanoseconds");
    if cycle_ns == 0 {
        return Duration::ZERO;
    }
    let phase = dc_system_time % cycle_ns;
    Duration::from_nanos((cycle_ns - phase) + landing_target_ns % cycle_ns)
}

fn overlapping_devices(offset: usize, len: usize, devices: usize) -> u16 {
    let chunk_end = offset + len;
    let stride = usize::from(OUTPUT_BYTES);
    let count = (0..devices)
        .filter(|index| {
            let start = index * stride;
            start < chunk_end && offset < start + stride
        })
        .count();
    u16::try_from(count).expect("device count fits in u16")
}

impl<B: RawBus> Master<B> {
    pub(crate) fn plan_cycle(&mut self) {
        let devices = self.devices;
        let expected = u16::try_from(devices).expect("device count fits in u16");
        let capacity = self.bus.mtu() - ECAT_HEADER_BYTES;

        let mut frames: Vec<Vec<PlannedDatagram>> = Vec::new();
        let mut current: Vec<PlannedDatagram> = vec![
            PlannedDatagram {
                command: Command::Frmw,
                address: Address::node(Self::station_address(0), reg::DC_SYSTEM_TIME),
                len: 8,
                expected_wkc: expected,
                role: Role::DcTime,
            },
            PlannedDatagram {
                command: Command::Brd,
                address: Address::broadcast(reg::AL_STATUS),
                len: 2,
                expected_wkc: expected,
                role: Role::AlStatus,
            },
            PlannedDatagram {
                command: Command::Fprd,
                address: Address::node(Self::station_address(0), reg::AL_STATUS),
                len: 2,
                expected_wkc: 1,
                role: Role::DeviceAlStatus,
            },
            PlannedDatagram {
                command: Command::Lrd,
                address: Address::Logical(INPUT_LOGICAL_BASE),
                len: devices * usize::from(INPUT_BYTES),
                expected_wkc: expected,
                role: Role::Inputs,
            },
        ];
        let mut used: usize = current
            .iter()
            .map(|d| DATAGRAM_OVERHEAD_BYTES + d.len)
            .sum();

        let total_outputs = devices * usize::from(OUTPUT_BYTES);
        let mut offset = 0usize;
        while offset < total_outputs {
            let available = capacity
                .saturating_sub(used)
                .saturating_sub(DATAGRAM_OVERHEAD_BYTES);
            if available == 0 {
                frames.push(std::mem::take(&mut current));
                used = 0;
                continue;
            }
            let len = available.min(total_outputs - offset);
            current.push(PlannedDatagram {
                command: Command::Lwr,
                address: Address::Logical(
                    OUTPUT_LOGICAL_BASE + u32::try_from(offset).expect("offset fits in u32"),
                ),
                len,
                expected_wkc: overlapping_devices(offset, len, devices),
                role: Role::Outputs { offset },
            });
            used += DATAGRAM_OVERHEAD_BYTES + len;
            offset += len;
        }
        if !current.is_empty() {
            frames.push(current);
        }

        let buffers = frames
            .iter()
            .map(|datagrams| {
                let bytes: usize = datagrams
                    .iter()
                    .map(|d| DATAGRAM_OVERHEAD_BYTES + d.len)
                    .sum();
                vec![
                    0u8;
                    (bytes + ECAT_HEADER_BYTES + ETH_HEADER_BYTES).max(MIN_ETHERNET_FRAME_BYTES)
                ]
            })
            .collect::<Vec<_>>();
        let slots = frames.iter().map(|d| Vec::with_capacity(d.len())).collect();
        let indices = vec![0u8; frames.len()];
        let sent = vec![0usize; frames.len()];
        let echo_pending = vec![true; frames.len()];
        let received = vec![false; frames.len()];

        tracing::debug!(
            frames = frames.len(),
            devices,
            "planned the cyclic process data exchange"
        );
        self.plan = CyclePlan {
            frames,
            buffers,
            slots,
            indices,
            sent,
            echo_pending,
            received,
        };
    }

    fn cyclic_deadline(&self) -> Duration {
        self.config.cycle.min(self.config.pdu_timeout)
    }

    fn account_for_al_status(&mut self, observed: bool) {
        if observed {
            self.unobserved_cycles = 0;
            return;
        }
        self.unobserved_cycles = self.unobserved_cycles.saturating_add(1);
        if self.unobserved_cycles == LOSE_CONTACT_AFTER_CYCLES {
            tracing::warn!(
                cycles = LOSE_CONTACT_AFTER_CYCLES,
                "no AL status came back for the last cycles; reporting the bus as lost",
            );
            self.state.lose_all();
        }
    }

    pub fn exchange(&mut self, tx: &[u8], rx: &mut [u8]) -> Result<CycleReport, EchocatError> {
        if self.plan.frames.is_empty() {
            return Err(EchocatError::Closed);
        }
        let observed = self.rotation;
        self.send_cycle(tx, observed)?;
        let report = self.collect_cycle(rx, observed)?;
        if self.devices > 0 {
            self.rotation = (self.rotation + 1) % self.devices;
        }
        Ok(report)
    }

    fn send_cycle(&mut self, tx: &[u8], observed: usize) -> Result<(), EchocatError> {
        for frame in 0..self.plan.frames.len() {
            self.index = self.index.wrapping_add(1);
            let index = self.index;
            self.plan.indices[frame] = index;
            self.plan.received[frame] = false;
            self.plan.echo_pending[frame] = true;
            self.plan.slots[frame].clear();

            let buffer = &mut self.plan.buffers[frame];
            let mut builder = FrameBuilder::new(buffer, index);
            for datagram in &self.plan.frames[frame] {
                let address = if datagram.role == Role::DeviceAlStatus {
                    Address::node(Self::station_address(observed), reg::AL_STATUS)
                } else {
                    datagram.address
                };
                let slot = builder.push(datagram.command, address, datagram.len)?;
                if let Role::Outputs { offset } = datagram.role {
                    builder
                        .data_mut(slot)
                        .copy_from_slice(&tx[offset..offset + datagram.len]);
                }
                self.plan.slots[frame].push(slot);
            }
            let len = builder.finish();
            self.plan.sent[frame] = len;
            self.bus.send(&self.plan.buffers[frame][..len])?;
        }
        Ok(())
    }

    fn collect_cycle(
        &mut self,
        rx: &mut [u8],
        observed: usize,
    ) -> Result<CycleReport, EchocatError> {
        let mut rx_valid = true;
        let mut dc_system_time = 0u64;
        let mut al_status = 0u16;
        let mut al_status_observed = false;
        let mut outstanding = self.plan.frames.len();
        let deadline = std::time::Instant::now() + self.cyclic_deadline();

        while outstanding > 0 {
            let now = std::time::Instant::now();
            if now >= deadline {
                rx_valid = false;
                break;
            }
            let Some(len) = self.bus.receive(&mut self.rx, deadline - now)? else {
                continue;
            };
            let Some(received_index) = self.rx.get(FRAME_HEADER_BYTES + 1).copied() else {
                continue;
            };
            let Some(frame) = self
                .plan
                .indices
                .iter()
                .position(|index| *index == received_index)
            else {
                continue;
            };
            if self.plan.received[frame] {
                continue;
            }
            if self.plan.echo_pending[frame]
                && super::is_echo(
                    &self.rx[..len],
                    &self.plan.buffers[frame][..self.plan.sent[frame]],
                )
            {
                tracing::trace!("discarding a frame the interface looped back");
                self.plan.echo_pending[frame] = false;
                continue;
            }
            let Ok(view) = FrameView::parse(&self.rx[..len], self.plan.indices[frame]) else {
                continue;
            };
            self.plan.received[frame] = true;
            outstanding -= 1;

            for (datagram, slot) in self.plan.frames[frame].iter().zip(&self.plan.slots[frame]) {
                let wkc = view.wkc(*slot)?;
                if wkc != datagram.expected_wkc {
                    rx_valid = false;
                }
                match datagram.role {
                    Role::DcTime => {
                        let data = view.data(*slot)?;
                        dc_system_time =
                            u64::from_le_bytes(data.try_into().expect("8 bytes of DC time"));
                    }
                    Role::AlStatus => {
                        let data = view.data(*slot)?;
                        al_status = u16::from_le_bytes([data[0], data[1]]);
                    }
                    Role::DeviceAlStatus => {
                        al_status_observed = true;
                        if wkc == datagram.expected_wkc {
                            let data = view.data(*slot)?;
                            self.state.observe(observed, data[0]);
                        } else {
                            self.state.lose(observed);
                        }
                    }
                    Role::Inputs => {
                        let data = view.data(*slot)?;
                        rx[..data.len()].copy_from_slice(data);
                    }
                    Role::Outputs { .. } => {}
                }
            }
        }
        if outstanding > 0 {
            rx_valid = false;
        }
        self.account_for_al_status(al_status_observed);

        Ok(CycleReport {
            rx_valid,
            dc_system_time,
            al_status,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{next_cycle_wait, overlapping_devices};
    use std::time::Duration;

    #[test]
    fn the_landing_phase_is_half_a_cycle_past_the_sync0_edge() {
        let cycle = Duration::from_millis(1);
        let target = 500_000;
        assert_eq!(
            next_cycle_wait(0, cycle, target),
            Duration::from_micros(1500),
            "at the latch edge the next landing is a cycle and a half away"
        );
        assert_eq!(
            next_cycle_wait(500_000, cycle, target),
            Duration::from_millis(1)
        );
        assert_eq!(
            next_cycle_wait(999_999, cycle, target),
            Duration::from_nanos(500_001)
        );
    }

    #[test]
    fn a_quarter_cycle_target_lands_a_quarter_cycle_past_the_sync0_edge() {
        let cycle = Duration::from_millis(2);
        let target = 500_000;
        assert_eq!(
            next_cycle_wait(0, cycle, target),
            Duration::from_micros(2500),
            "the wait carries to the next edge and then to the target"
        );
        assert_eq!(
            next_cycle_wait(500_000, cycle, target),
            Duration::from_millis(2),
            "landing on the target asks for exactly one more cycle"
        );
    }

    #[test]
    fn the_landing_phase_never_bunches_two_frames_into_one_sync0_period() {
        let cycle = Duration::from_millis(1);
        for target in [1u64, 250_000, 500_000, 999_999] {
            for phase in (0..1_000_000).step_by(9_973) {
                let wait = next_cycle_wait(phase, cycle, target);
                assert!(
                    wait > Duration::ZERO && wait <= cycle * 2,
                    "wait {wait:?} for phase {phase} and target {target} escapes (0, 2 cycles]"
                );
            }
        }
    }

    #[test]
    fn output_chunks_expect_a_working_counter_from_every_device_they_touch() {
        assert_eq!(overlapping_devices(0, 626, 4), 1);
        assert_eq!(overlapping_devices(0, 1252, 4), 2);
        assert_eq!(overlapping_devices(600, 100, 4), 2);
        assert_eq!(overlapping_devices(1252, 1252, 4), 2);
        assert_eq!(overlapping_devices(0, 626 * 4, 4), 4);
    }
}
