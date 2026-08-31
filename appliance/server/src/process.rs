#[cfg(target_os = "linux")]
pub fn lock_memory() {
    let rc = unsafe { libc::mlockall(libc::MCL_CURRENT | libc::MCL_FUTURE) };
    if rc == 0 {
        tracing::debug!("locked the process pages into RAM");
    } else {
        tracing::warn!(
            "failed to lock the process pages into RAM: {}. \
             A page fault during a bus cycle can drop the devices to SAFE-OP. \
             Grant the capability with `sudo setcap cap_ipc_lock+ep <executable>`, \
             or raise `memlock` for the user in /etc/security/limits.conf",
            std::io::Error::last_os_error(),
        );
    }
}

#[cfg(not(target_os = "linux"))]
pub fn lock_memory() {
    tracing::debug!("memory locking is only implemented on Linux; skipping");
}
