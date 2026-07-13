use std::path::Path;

use anyhow::{Context, Result};
use clap::Args;

const EMULATOR_CRATE: &str = "crates/autd3-rs-firmware-emulator";

#[derive(Args)]
pub struct VendorFwCmd {}

pub fn run_vendor_fw(root: &Path, _cmd: &VendorFwCmd) -> Result<()> {
    crate::cpu::gen_param(root)?;

    let src = root.join("firmware/cpu/fw/src");
    let dst = root.join(EMULATOR_CRATE).join("vendor/cpu-fw");

    if dst.exists() {
        std::fs::remove_dir_all(&dst)
            .with_context(|| format!("failed to clear {}", dst.display()))?;
    }
    copy_dir(&src, &dst)?;
    println!("Vendored firmware/cpu/fw/src -> {}", dst.display());
    Ok(())
}

fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst).with_context(|| format!("failed to create {}", dst.display()))?;
    for entry in
        std::fs::read_dir(src).with_context(|| format!("failed to read {}", src.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        // The firmware's own unit tests are not part of the emulator build.
        if name == "tests" {
            continue;
        }
        let to = dst.join(&name);
        if path.is_dir() {
            copy_dir(&path, &to)?;
        } else {
            std::fs::copy(&path, &to).with_context(|| {
                format!("failed to copy {} -> {}", path.display(), to.display())
            })?;
        }
    }
    Ok(())
}
