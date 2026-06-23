use std::path::Path;

use anyhow::{Context, Result};
use clap::Args;

const EMULATOR_CRATE: &str = "crates/autd3-rs-firmware-emulator";

/// Copy the CPU firmware sources into the firmware-emulator crate so that a
/// published package is self-contained (the canonical source stays in
/// `firmware/cpu`). `build.rs` prefers this vendored copy when present.
#[derive(Args)]
pub struct VendorFwCmd {}

pub fn run_vendor_fw(root: &Path, _cmd: &VendorFwCmd) -> Result<()> {
    let src = root.join("firmware/cpu");
    let dst = root.join(EMULATOR_CRATE).join("vendor/cpu");

    if dst.exists() {
        std::fs::remove_dir_all(&dst)
            .with_context(|| format!("failed to clear {}", dst.display()))?;
    }
    for sub in ["inc", "src"] {
        copy_dir(&src.join(sub), &dst.join(sub))?;
    }
    println!("Vendored firmware/cpu -> {}", dst.display());
    Ok(())
}

fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst).with_context(|| format!("failed to create {}", dst.display()))?;
    for entry in std::fs::read_dir(src).with_context(|| format!("failed to read {}", src.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let to = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir(&path, &to)?;
        } else {
            std::fs::copy(&path, &to)
                .with_context(|| format!("failed to copy {} -> {}", path.display(), to.display()))?;
        }
    }
    Ok(())
}
