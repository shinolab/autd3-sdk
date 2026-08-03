use std::path::PathBuf;

pub const TWINCAT_SUBDIR: &str = "twincat";
pub const TWINCAT_BIN: &str = "twincat-cli";

#[cfg(windows)]
const TWINCAT_FILES: [&str; 2] = ["twincat-cli.exe.config", "twincat-cli.exe"];

#[cfg(windows)]
#[derive(rust_embed::RustEmbed)]
#[folder = "twincat/"]
struct TwinCatAssets;

pub fn tool_bin(subdir: &str, name: &str) -> std::io::Result<PathBuf> {
    let exe_name = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    let dir = exe_dir()?;
    let adjacent = dir.join(&exe_name);
    if adjacent.is_file() {
        return Ok(adjacent);
    }
    Ok(dir.join(subdir).join(exe_name))
}

pub fn twincat_bin() -> std::io::Result<PathBuf> {
    let found = tool_bin(TWINCAT_SUBDIR, TWINCAT_BIN)?;
    if found.is_file() {
        return Ok(found);
    }
    #[cfg(windows)]
    if let Some(extracted) = extract_twincat()? {
        return Ok(extracted);
    }
    Ok(found)
}

#[cfg(windows)]
fn extract_twincat() -> std::io::Result<Option<PathBuf>> {
    let exe_name = TWINCAT_FILES[TWINCAT_FILES.len() - 1];
    if TwinCatAssets::get(exe_name).is_none() {
        return Ok(None);
    }

    let dir = twincat_cache_dir()?;
    let exe = dir.join(exe_name);
    if exe.is_file() {
        return Ok(Some(exe));
    }

    std::fs::create_dir_all(&dir)?;
    for name in TWINCAT_FILES {
        let Some(file) = TwinCatAssets::get(name) else {
            continue;
        };
        let tmp = dir.join(format!("{name}.tmp"));
        std::fs::write(&tmp, file.data.as_ref())?;
        std::fs::rename(&tmp, dir.join(name))?;
    }
    Ok(Some(exe))
}

#[cfg(windows)]
fn twincat_cache_dir() -> std::io::Result<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA").ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "%LOCALAPPDATA% is not set, so the bundled twincat-cli cannot be unpacked",
        )
    })?;
    Ok(PathBuf::from(local)
        .join("autd3")
        .join(TWINCAT_SUBDIR)
        .join(env!("CARGO_PKG_VERSION")))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn the_bundled_twincat_cli_is_unpacked_when_it_is_embedded() {
        let exe_name = TWINCAT_FILES[TWINCAT_FILES.len() - 1];
        if TwinCatAssets::get(exe_name).is_none() {
            return;
        }

        let dir = twincat_cache_dir().unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        let bin = twincat_bin().unwrap();
        assert_eq!(bin, dir.join(exe_name));
        assert!(bin.is_file());
        for name in TWINCAT_FILES {
            assert!(dir.join(name).is_file(), "{name} was not unpacked");
        }
        assert!(std::fs::read_dir(&dir).unwrap().all(|e| {
            let name = e.unwrap().file_name();
            TWINCAT_FILES.iter().any(|f| name == **f)
        }));

        let stamp = std::fs::metadata(&bin).unwrap().modified().unwrap();
        assert_eq!(twincat_bin().unwrap(), bin);
        assert_eq!(std::fs::metadata(&bin).unwrap().modified().unwrap(), stamp);
    }
}

fn exe_dir() -> std::io::Result<PathBuf> {
    let exe = std::env::current_exe()?;
    exe.parent().map(PathBuf::from).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "could not resolve the console executable directory",
        )
    })
}
