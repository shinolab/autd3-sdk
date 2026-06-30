use std::ffi::OsStr;
use std::path::Path;

use anyhow::{Context, Result, bail};

pub fn on_path(name: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string())
            .split(';')
            .map(str::to_string)
            .collect()
    } else {
        vec![String::new()]
    };
    std::env::split_paths(&paths).any(|dir| {
        exts.iter()
            .any(|ext| dir.join(format!("{name}{ext}")).is_file())
    })
}

pub fn run<I, S>(program: &str, args: I, cwd: &Path) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let status = std::process::Command::new(program)
        .args(args)
        .current_dir(cwd)
        .status()
        .with_context(|| format!("failed to spawn `{program}` (is it installed and on PATH?)"))?;
    if !status.success() {
        bail!("`{program}` exited with {status}");
    }
    Ok(())
}
