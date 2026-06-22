use std::future::Future;
use std::time::{Duration, Instant};

use ethercrab::std::ethercat_now;
use ethercrab::subdevice_group::{HasDc, NoDc, Op, PreOp, SafeOp};
use ethercrab::{Command, DcSync, MainDevice, RegisterAddress};
use tokio::runtime::Handle;

use crate::diagnostics::new_shared_cycle_diagnostics;
use crate::error::EtherCrabLinkError;
use crate::option::EtherCrabLinkOptionFull;
use crate::timer;
use crate::transport::Transport;
use crate::{sync, utils};

use super::{
    EtherCrabLink, GRACEFUL_SHUTDOWN_TIMEOUT, GROUP_SUBDEVICES, Groups, MAX_GROUPS, MAX_SUBDEVICES,
    OP_WAIT_TIMEOUT, OP_WARMUP_CYCLES, OP_WKC_STABLE_CYCLES, SUBDEVICE_NAME, SubGroup,
};

pub(super) struct Reached {
    pub(super) group: Groups<Op, HasDc>,
    pub(super) addresses: Vec<u16>,
    pub(super) expected_wkc: u16,
    pub(super) num_devices: usize,
}

impl EtherCrabLink {
    pub async fn open(
        option: impl Into<EtherCrabLinkOptionFull>,
    ) -> Result<Self, EtherCrabLinkError> {
        let option: EtherCrabLinkOptionFull = option.into();
        tracing::debug!(?option, "opening EtherCrabLink");

        let timer_resolution = crate::timer::TimerResolutionGuard::new(super::TIMER_RESOLUTION_MS);
        let handle = Handle::try_current().map_err(|_| EtherCrabLinkError::NoTokioRuntime)?;

        let interface = if let Some(interface) = option.interface.name() {
            interface.to_owned()
        } else {
            tracing::info!("no interface specified, looking for AUTD devices");
            let interface = Box::pin(utils::lookup_autd()).await?;
            tracing::info!("found AUTD devices on {interface}");
            interface
        };

        let diagnostics = new_shared_cycle_diagnostics();
        tracing::info!("starting EtherCAT tx/rx task on {interface}");
        let transport = Transport::open(
            &handle,
            &interface,
            option.timeouts,
            option.main_device_config,
        )?;
        let Reached {
            group,
            addresses,
            expected_wkc,
            num_devices,
        } = Box::pin(try_reach_op(transport.maindevice(), &option, &interface)).await?;

        Ok(Self {
            group: Some(group),
            addresses,
            transport,
            handle,
            next_at: None,
            num_devices,
            expected_wkc,
            rx_was_valid: true,
            stats: autd3_rs_core::LinkStats::default(),
            diagnostics,
            _timer_resolution: timer_resolution,
        })
    }

    pub async fn close(mut self) -> Result<(), EtherCrabLinkError> {
        match self.group.take() {
            Some(group) => Box::pin(shutdown(group, self.transport.maindevice())).await,
            None => Ok(()),
        }
    }

    #[must_use]
    pub fn num_devices(&self) -> usize {
        self.num_devices
    }
}

impl Drop for EtherCrabLink {
    fn drop(&mut self) {
        let Some(group) = self.group.take() else {
            return;
        };
        if Handle::try_current().is_ok() {
            tracing::warn!(
                "EtherCrabLink dropped inside an async context; skipping the INIT transition \
                 (use EtherCrabLink::close for a graceful shutdown)",
            );
            return;
        }
        let maindevice = self.transport.maindevice();
        match self.handle.block_on(Box::pin(run_with_timeout(
            GRACEFUL_SHUTDOWN_TIMEOUT,
            shutdown(group, maindevice),
        ))) {
            Ok(Ok(())) => {}
            Ok(Err(e)) => tracing::warn!("graceful shutdown failed: {e}"),
            Err(_) => tracing::warn!(
                "graceful shutdown timed out after {GRACEFUL_SHUTDOWN_TIMEOUT:?}; forcing teardown"
            ),
        }
    }
}

async fn shutdown(
    group: Groups<Op, HasDc>,
    maindevice: &MainDevice<'static>,
) -> Result<(), EtherCrabLinkError> {
    tracing::info!("transitioning devices to INIT");
    let group = Box::pin(group.transform(|g| g.into_safe_op(maindevice))).await?;
    let group = Box::pin(group.transform(|g| g.into_pre_op(maindevice))).await?;
    let _ = Box::pin(group.transform(|g| g.into_init(maindevice))).await?;
    tracing::info!("all devices are in INIT");
    Ok(())
}

async fn run_with_timeout<F>(
    timeout: Duration,
    future: F,
) -> Result<F::Output, tokio::time::error::Elapsed>
where
    F: Future,
{
    tokio::time::timeout(timeout, future).await
}

async fn try_reach_op(
    maindevice: &MainDevice<'static>,
    option: &EtherCrabLinkOptionFull,
    interface: &str,
) -> Result<Reached, EtherCrabLinkError> {
    #[derive(Default)]
    struct GroupsArray {
        groups: [SubGroup<PreOp, NoDc>; MAX_GROUPS],
    }
    let mut idx = 0usize;
    let groups = Box::pin(maindevice.init::<MAX_SUBDEVICES, _>(
        ethercat_now,
        GroupsArray::default(),
        |array: &GroupsArray, _subdevice| {
            let group = &array.groups[idx / GROUP_SUBDEVICES];
            idx += 1;
            Ok(group)
        },
    ))
    .await?;
    let groups = Groups {
        groups: groups
            .groups
            .into_iter()
            .filter(|g| !g.is_empty())
            .collect::<Vec<_>>(),
    };
    if groups.groups.is_empty() {
        return Err(EtherCrabLinkError::DeviceNotFound);
    }
    for (index, subdevice) in groups
        .groups
        .iter()
        .flat_map(|g| g.iter(maindevice))
        .enumerate()
    {
        if subdevice.name() != SUBDEVICE_NAME {
            return Err(EtherCrabLinkError::NotAutdDevice {
                index,
                name: subdevice.name().to_string(),
            });
        }
    }
    let num_devices = groups.num_devices();
    tracing::info!("found {num_devices} AUTD device(s) on {interface}");

    let mut groups = groups;
    for mut subdevice in groups
        .groups
        .iter_mut()
        .flat_map(|g| g.iter_mut(maindevice))
    {
        subdevice.set_dc_sync(DcSync::Sync0);
    }

    tracing::info!("moving into PRE-OP with PDI");
    let group = Box::pin(groups.transform(|g| g.into_pre_op_pdi(maindevice))).await?;

    sync::wait_for_align(
        &group,
        maindevice,
        option.sync_tolerance,
        option.sync_timeout,
    )
    .await?;

    tracing::info!(sync0_period = ?option.dc_configuration.sync0_period, "configuring Sync0");
    let group =
        Box::pin(group.transform(|g| g.configure_dc_sync(maindevice, option.dc_configuration)))
            .await?;

    let group = Box::pin(group.transform(|g| g.into_safe_op(maindevice))).await?;
    tracing::info!("all devices are in SAFE-OP");

    tracing::info!(
        cycles = OP_WARMUP_CYCLES,
        "warming up DC before requesting OP"
    );
    Box::pin(warmup_dc(&group, maindevice)).await?;

    let group = Box::pin(group.transform(|g| g.request_into_op(maindevice))).await?;
    tracing::info!("requested OP, waiting for all devices");

    let expected_wkc = wait_for_op(&group, maindevice).await?;
    let addresses: Vec<u16> = group
        .groups
        .iter()
        .flat_map(|g| g.iter(maindevice))
        .map(|subdevice| subdevice.configured_address())
        .collect();

    Ok(Reached {
        group,
        addresses,
        expected_wkc,
        num_devices,
    })
}

async fn warmup_dc(
    group: &Groups<SafeOp, HasDc>,
    maindevice: &MainDevice<'_>,
) -> Result<(), EtherCrabLinkError> {
    for _ in 0..OP_WARMUP_CYCLES {
        let cycle_start = Instant::now();
        let resp = group.tx_rx_dc(maindevice).await?;
        timer::async_sleep_until(cycle_start + resp.next_cycle_wait).await;
    }
    Ok(())
}

async fn wait_for_op(
    group: &Groups<Op, HasDc>,
    maindevice: &MainDevice<'_>,
) -> Result<u16, EtherCrabLinkError> {
    let op_requested = Instant::now();
    let op_deadline = op_requested + OP_WAIT_TIMEOUT;
    let mut baseline_wkc: Option<u16> = None;
    let mut stable_cycles: u32 = 0;
    let mut last_log = op_requested;
    loop {
        let cycle_start = Instant::now();
        let resp = group.tx_rx_dc(maindevice).await?;
        if last_log.elapsed() >= Duration::from_secs(1) {
            last_log = cycle_start;
            tracing::info!(
                all_op = resp.all_op,
                working_counter = resp.working_counter,
                "waiting for OP",
            );
        }
        if resp.all_op && baseline_wkc == Some(resp.working_counter) {
            stable_cycles += 1;
            if stable_cycles >= OP_WKC_STABLE_CYCLES {
                let expected_wkc = resp.working_counter;
                tracing::info!(
                    expected_wkc,
                    elapsed = ?op_requested.elapsed(),
                    "all devices entered OP",
                );
                return Ok(expected_wkc);
            }
        } else if resp.all_op {
            tracing::debug!(
                working_counter = resp.working_counter,
                "devices are in OP but wkc is not stable yet",
            );
            baseline_wkc = Some(resp.working_counter);
            stable_cycles = 1;
        } else {
            tracing::trace!(
                working_counter = resp.working_counter,
                "devices are not in OP yet",
            );
            baseline_wkc = None;
            stable_cycles = 0;
        }
        if Instant::now() >= op_deadline {
            tracing::error!(
                elapsed = ?op_requested.elapsed(),
                "timeout waiting for OP: devices did not reach OP within {OP_WAIT_TIMEOUT:?}",
            );
            log_al_status(group, maindevice).await;
            return Err(EtherCrabLinkError::OpTimeout);
        }
        timer::async_sleep_until(cycle_start + resp.next_cycle_wait).await;
    }
}

async fn log_al_status(group: &Groups<Op, HasDc>, maindevice: &MainDevice<'_>) {
    let addresses: Vec<u16> = group
        .groups
        .iter()
        .flat_map(|g| g.iter(maindevice))
        .map(|subdevice| subdevice.configured_address())
        .collect();
    for (index, address) in addresses.into_iter().enumerate() {
        let status = Command::fprd(address, RegisterAddress::AlStatus.into())
            .receive::<u16>(maindevice)
            .await;
        let code = Command::fprd(address, RegisterAddress::AlStatusCode.into())
            .receive::<u16>(maindevice)
            .await;
        if let (Ok(status), Ok(code)) = (status, code) {
            tracing::error!(
                index,
                state = al_state_str(status),
                error = status & 0x0010 != 0,
                al_status = format_args!("{status:#06x}"),
                al_status_code = format_args!("{code:#06x}"),
                reason = al_status_code_str(code),
                "device did not reach OP",
            );
        } else {
            tracing::error!(index, address, "failed to read AL status for diagnostics");
        }
    }
}

fn al_state_str(status: u16) -> &'static str {
    match status & 0x000F {
        1 => "INIT",
        2 => "PRE-OP",
        3 => "BOOT",
        4 => "SAFE-OP",
        8 => "OP",
        _ => "UNKNOWN",
    }
}

fn al_status_code_str(code: u16) -> &'static str {
    match code {
        0x0000 => "no error",
        0x0011 => "invalid requested state change",
        0x0012 => "unknown requested state",
        0x0017 => "invalid sync manager configuration",
        0x0018 => "no valid inputs available",
        0x0019 => "no valid outputs",
        0x001A => "synchronization error",
        0x001B => "sync manager watchdog",
        0x001D => "invalid output configuration",
        0x001E => "invalid input configuration",
        0x0024 => "invalid input mapping",
        0x0025 => "invalid output mapping",
        0x0028 => "sync mode not supported",
        0x002C => "fatal sync error",
        0x002D => "no sync error (sync signal missing)",
        0x0030 => "invalid DC SYNC configuration",
        0x0031 => "invalid DC latch configuration",
        0x0032 => "DC PLL error",
        0x0033 => "DC sync IO error",
        0x0034 => "DC sync timeout error",
        0x0035 => "DC invalid sync cycle time",
        0x0036 => "DC Sync0 cycle time",
        0x0037 => "DC Sync1 cycle time",
        _ => "see ETG.1000 AL status codes",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_future_can_be_created_outside_runtime() {
        let future = run_with_timeout(Duration::from_millis(1), async { 42u8 });
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        assert_eq!(rt.block_on(future).unwrap(), 42);
    }
}
