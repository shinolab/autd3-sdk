use std::ffi::CStr;

use crate::bindings::{ec_find_adapters, ec_free_adapters};
use crate::context::Context;
use crate::error::SoemLinkError;
use crate::link::SUBDEVICE_NAME;
use crate::option::SoemLinkOptionFull;

fn list_interfaces() -> Vec<String> {
    let mut interfaces = Vec::new();
    // SAFETY: `ec_find_adapters` returns an owned NUL-terminated linked
    // list, freed exactly once by `ec_free_adapters` below.
    unsafe {
        let head = ec_find_adapters();
        let mut adapter = head;
        while !adapter.is_null() {
            if let Ok(name) = CStr::from_ptr((*adapter).name.as_ptr()).to_str() {
                interfaces.push(name.to_string());
            }
            adapter = (*adapter).next;
        }
        ec_free_adapters(head);
    }
    interfaces
}

pub(crate) fn lookup_autd(option: &SoemLinkOptionFull) -> Result<String, SoemLinkError> {
    let interfaces = list_interfaces();
    tracing::debug!("found {} network interfaces", interfaces.len());
    for interface in interfaces {
        tracing::debug!("searching AUTD devices on {interface}");
        let ctx = Context::new(option.sync0_period, option.sync0_shift);
        if ctx.init(&interface).is_err() {
            tracing::trace!("failed to initialize SOEM on {interface}");
            continue;
        }
        let num_slaves = ctx.config_init();
        tracing::trace!("found {num_slaves} EtherCAT subdevice(s) on {interface}");
        if num_slaves > 0 && (0..num_slaves).all(|index| ctx.slave_name(index) == SUBDEVICE_NAME) {
            return Ok(interface);
        }
    }
    Err(SoemLinkError::DeviceNotFound)
}
