use std::io::Read;
use std::path::{Path, PathBuf};

use crate::series::Series;

const FPGA_UPDATE_SUFFIX: &str = "-fpga-update.img";

#[derive(Debug, thiserror::Error)]
pub enum BundleError {
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

pub fn fetch(version: &str, force_download: bool, series: Series) -> Result<Bundle> {
    let version = version.trim_start_matches('v');
    let dir = download_and_extract(version, force_download, series)?;
    find(&dir)
}

fn download_and_extract(version: &str, force: bool, series: Series) -> Result<PathBuf> {
    let dest = std::env::temp_dir().join(series.cache_dir_name(version));
    if dest.is_dir() && !force {
        eprintln!("Using cached firmware at {}", dest.display());
        return Ok(dest);
    }
    if dest.exists() {
        std::fs::remove_dir_all(&dest).map_err(io_err("removing stale cache", &dest))?;
    }

    let url = series.bundle_url(version);
    eprintln!("Downloading {url}");
    let resp = ureq::get(&url)
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

    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(BundleError::Open)?;
    archive
        .extract(&dest)
        .map_err(|source| BundleError::Extract {
            path: dest.clone(),
            source,
        })?;
    Ok(dest)
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
    use super::*;

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
        let dir = scratch("legacy");
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
