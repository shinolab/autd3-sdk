use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

const FPGA_UPDATE_SUFFIX: &str = "-fpga-update.img";

#[derive(Debug, thiserror::Error)]
pub enum BundleError {
    #[error("`{version}` is not a release version (expected MAJOR.MINOR.PATCH, e.g. 0.10.0)")]
    InvalidVersion { version: String },
    #[error(
        "release {}.{}.{} is an EtherCAT firmware; only UDP firmware ({}.{}.{} or newer) is supported",
        release[0], release[1], release[2],
        MIN_CPU_FIRMWARE_VERSION[0], MIN_CPU_FIRMWARE_VERSION[1], MIN_CPU_FIRMWARE_VERSION[2]
    )]
    UnsupportedVersion { release: [u8; 3] },
    #[error("{action} {}", path.display())]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("downloading {url}")]
    Download {
        url: String,
        #[source]
        source: Box<ureq::Error>,
    },
    #[error("reading firmware bundle")]
    Read(#[source] std::io::Error),
    #[error("opening firmware zip (was the version correct?)")]
    Open(#[source] zip::result::ZipError),
    #[error("extracting firmware to {}", path.display())]
    Extract {
        path: PathBuf,
        #[source]
        source: zip::result::ZipError,
    },
    #[error("no firmware images (*.bin / *.mcs / *{FPGA_UPDATE_SUFFIX}) found in {}", dir.display())]
    NoImages { dir: PathBuf },
    #[error("multiple {pattern} firmware images found in {}; cannot tell which one to write:\n{list}", dir.display())]
    Ambiguous {
        pattern: String,
        dir: PathBuf,
        list: String,
    },
}

type Result<T> = std::result::Result<T, BundleError>;

fn io_err(action: &'static str, path: &Path) -> impl FnOnce(std::io::Error) -> BundleError {
    let path = path.to_path_buf();
    move |source| BundleError::Io {
        action,
        path,
        source,
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Bundle {
    pub cpu: Option<PathBuf>,
    pub fpga_mcs: Option<PathBuf>,
    pub fpga_update: Option<PathBuf>,
}

pub const MIN_CPU_FIRMWARE_VERSION: [u8; 3] = [0, 10, 0];

pub fn parse_release(version: &str) -> Result<[u8; 3]> {
    fn parse(version: &str) -> Option<[u8; 3]> {
        let mut parts = version.trim_start_matches('v').split('.');
        let mut next = || parts.next()?.parse::<u8>().ok();
        let release = [next()?, next()?, next()?];
        parts.next().is_none().then_some(release)
    }

    let release = parse(version).ok_or_else(|| BundleError::InvalidVersion {
        version: version.to_string(),
    })?;
    if release < MIN_CPU_FIRMWARE_VERSION {
        return Err(BundleError::UnsupportedVersion { release });
    }
    Ok(release)
}

#[must_use]
pub fn bundle_url(version: &str) -> String {
    format!(
        "https://github.com/shinolab/autd3-sdk/releases/download/firmware-v{version}/autd3-sdk-firmware-v{version}.zip"
    )
}

pub fn fetch([major, minor, patch]: [u8; 3], force_download: bool) -> Result<Bundle> {
    let version = format!("{major}.{minor}.{patch}");
    let temp = std::env::temp_dir();
    let dest = temp.join(format!("autd3-firmware-bundle-v{version}"));
    if dest.is_dir() && !force_download {
        eprintln!("Using cached firmware at {}", dest.display());
    } else {
        let url = bundle_url(&version);
        eprintln!("Downloading {url}");
        let resp = ureq::get(&url)
            .config()
            .timeout_global(Some(Duration::from_secs(600)))
            .build()
            .call()
            .map_err(|source| BundleError::Download {
                url: url.clone(),
                source: Box::new(source),
            })?;
        let mut bytes = Vec::new();
        resp.into_body()
            .into_reader()
            .read_to_end(&mut bytes)
            .map_err(BundleError::Read)?;

        let staging = temp.join(format!(
            "autd3-firmware-bundle-v{version}.{}.partial",
            std::process::id()
        ));
        install(bytes, &staging, &dest)?;
    }
    find(&dest)
}

fn install(bytes: Vec<u8>, staging: &Path, dest: &Path) -> Result<()> {
    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(BundleError::Open)?;
    let _ = std::fs::remove_dir_all(staging);
    let installed = archive
        .extract(staging)
        .map_err(|source| BundleError::Extract {
            path: staging.to_path_buf(),
            source,
        })
        .and_then(|()| {
            if dest.exists() {
                std::fs::remove_dir_all(dest).map_err(io_err("removing stale cache", dest))?;
            }
            std::fs::rename(staging, dest).map_err(io_err("publishing firmware to", dest))
        });
    if installed.is_err() {
        let _ = std::fs::remove_dir_all(staging);
    }
    installed
}

pub fn find(dir: &Path) -> Result<Bundle> {
    let mut files = Vec::new();
    collect_files(dir, &mut files)?;

    let mut cpu = Vec::new();
    let mut fpga_mcs = Vec::new();
    let mut fpga_update = Vec::new();
    for f in files {
        let name = f
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if name.ends_with(FPGA_UPDATE_SUFFIX) {
            fpga_update.push(f);
            continue;
        }
        match f.extension().and_then(|e| e.to_str()) {
            Some("bin") => cpu.push(f),
            Some("mcs") => fpga_mcs.push(f),
            _ => {}
        }
    }
    if cpu.is_empty() && fpga_mcs.is_empty() && fpga_update.is_empty() {
        return Err(BundleError::NoImages {
            dir: dir.to_path_buf(),
        });
    }
    Ok(Bundle {
        cpu: single(cpu, "*.bin", dir)?,
        fpga_mcs: single(fpga_mcs, "*.mcs", dir)?,
        fpga_update: single(fpga_update, &format!("*{FPGA_UPDATE_SUFFIX}"), dir)?,
    })
}

fn single(mut candidates: Vec<PathBuf>, pattern: &str, dir: &Path) -> Result<Option<PathBuf>> {
    if candidates.len() > 1 {
        candidates.sort();
        let list = candidates
            .iter()
            .map(|p| format!("  {}", p.display()))
            .collect::<Vec<_>>()
            .join("\n");
        return Err(BundleError::Ambiguous {
            pattern: pattern.to_string(),
            dir: dir.to_path_buf(),
            list,
        });
    }
    Ok(candidates.pop())
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries = std::fs::read_dir(dir).map_err(io_err("reading", dir))?;
    for entry in entries {
        let path = entry.map_err(io_err("reading", dir))?.path();
        if path.is_dir() {
            collect_files(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn zipped(name: &str, content: &[u8]) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let stored = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        zip.start_file(name, stored).unwrap();
        zip.write_all(content).unwrap();
        zip.finish().unwrap().into_inner()
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "autd3-firmware-writer-bundle-test-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("nested")).unwrap();
        dir
    }

    #[test]
    fn a_release_parses_with_or_without_the_v_prefix() {
        assert_eq!(parse_release("0.10.0").unwrap(), [0, 10, 0]);
        assert_eq!(parse_release("v1.2.3").unwrap(), [1, 2, 3]);
    }

    #[test]
    fn ethercat_releases_are_rejected() {
        for older in ["0.9.0", "v0.9.9", "0.6.1", "0.0.0"] {
            let err = parse_release(older).unwrap_err().to_string();
            assert!(err.contains("EtherCAT") && err.contains("0.10.0"), "{err}");
        }
    }

    #[test]
    fn a_version_that_is_not_three_numbers_is_rejected() {
        for malformed in [
            "",
            "latest",
            "0.10",
            "0.10.0.1",
            "0.10.0-rc.1",
            "0.13.0/../../home",
            "../x",
            "0.13.0/autd3-sdk-firmware-v0.13.0.zip#",
        ] {
            let err = parse_release(malformed).unwrap_err().to_string();
            assert!(err.contains("MAJOR.MINOR.PATCH"), "{err}");
        }
    }

    #[test]
    fn a_corrupt_bundle_leaves_the_cache_untouched() {
        let dir = scratch("corrupt-bundle");
        let (staging, dest) = (dir.join("staging"), dir.join("dest"));
        std::fs::create_dir(&dest).unwrap();
        std::fs::write(dest.join("old.bin"), b"old").unwrap();
        let mut bytes = zipped("new.bin", &[0xAA; 256]);
        bytes[64] ^= 0xFF;

        assert!(matches!(
            install(bytes, &staging, &dest),
            Err(BundleError::Extract { .. })
        ));

        assert!(!staging.exists());
        assert_eq!(std::fs::read(dest.join("old.bin")).unwrap(), b"old");
        assert!(!dest.join("new.bin").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_truncated_download_publishes_nothing() {
        let dir = scratch("truncated-bundle");
        let (staging, dest) = (dir.join("staging"), dir.join("dest"));
        let mut bytes = zipped("new.bin", &[0xAA; 256]);
        bytes.truncate(bytes.len() / 2);

        assert!(matches!(
            install(bytes, &staging, &dest),
            Err(BundleError::Open(_))
        ));

        assert!(!staging.exists() && !dest.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_good_bundle_replaces_the_cache() {
        let dir = scratch("good-bundle");
        let (staging, dest) = (dir.join("staging"), dir.join("dest"));
        std::fs::create_dir(&dest).unwrap();
        std::fs::write(dest.join("old.bin"), b"old").unwrap();

        install(zipped("new.bin", b"new"), &staging, &dest).unwrap();

        assert!(!staging.exists() && !dest.join("old.bin").exists());
        assert_eq!(std::fs::read(dest.join("new.bin")).unwrap(), b"new");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_fpga_update_image_is_not_mistaken_for_anything_else() {
        let dir = scratch("classify");
        for name in [
            "autd3-sdk-firmware-v0.9.0.bin",
            "autd3-sdk-firmware-v0.9.0.mcs",
            "nested/autd3-sdk-firmware-v0.9.0-fpga-update.img",
            "README.txt",
        ] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        let bundle = find(&dir).unwrap();
        assert_eq!(
            bundle,
            Bundle {
                cpu: Some(dir.join("autd3-sdk-firmware-v0.9.0.bin")),
                fpga_mcs: Some(dir.join("autd3-sdk-firmware-v0.9.0.mcs")),
                fpga_update: Some(dir.join("nested/autd3-sdk-firmware-v0.9.0-fpga-update.img")),
            }
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_bundle_without_an_update_image_still_resolves() {
        let dir = scratch("no-update-image");
        std::fs::write(dir.join("fw.bin"), b"x").unwrap();
        std::fs::write(dir.join("fw.mcs"), b"x").unwrap();
        let bundle = find(&dir).unwrap();
        assert_eq!(bundle.fpga_update, None);
        assert!(bundle.cpu.is_some() && bundle.fpga_mcs.is_some());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn ambiguous_images_are_rejected() {
        let dir = scratch("ambiguous");
        std::fs::write(dir.join("a.bin"), b"x").unwrap();
        std::fs::write(dir.join("nested/b.bin"), b"x").unwrap();
        assert!(find(&dir).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
