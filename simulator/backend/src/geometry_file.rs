use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs_simulator_protocol::DevicePose;

use crate::emulator::{device_poses, geometry_from_poses};

pub(crate) fn default_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("autd3-simulator").join("geometry.json"))
}

pub(crate) fn load(path: &Path) -> Result<Geometry> {
    let json = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let poses: Vec<DevicePose> =
        serde_json::from_str(&json).with_context(|| format!("decoding {}", path.display()))?;
    geometry_from_poses(&poses).with_context(|| format!("checking {}", path.display()))
}

pub(crate) fn load_or_default(path: Option<&Path>) -> Geometry {
    let Some(path) = path else {
        return Geometry::new(vec![Autd3::default()]);
    };
    load(path).unwrap_or_else(|e| {
        tracing::warn!(
            "could not load the saved geometry ({e:#}); starting with one AUTD3 at the origin"
        );
        Geometry::new(vec![Autd3::default()])
    })
}

pub(crate) fn save(path: &Path, geometry: &Geometry) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_vec_pretty(&device_poses(geometry))?;
    let staging = staging_path(path);
    let written = fs::File::create(&staging)
        .and_then(|mut file| {
            file.write_all(&json)?;
            file.sync_all()
        })
        .and_then(|()| fs::rename(&staging, path));
    if written.is_err() {
        let _ = fs::remove_file(&staging);
    }
    written
}

fn staging_path(path: &Path) -> PathBuf {
    path.with_extension(format!("json.{}.tmp", std::process::id()))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    use autd3_rs::geometry::{Point3, UnitQuaternion};

    pub(crate) struct ScratchDir(PathBuf);

    impl ScratchDir {
        pub(crate) fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "autd3-simulator-test-{}-{name}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&dir);
            Self(dir)
        }

        pub(crate) fn file(&self) -> PathBuf {
            self.0.join("nested").join("geometry.json")
        }
    }

    impl Drop for ScratchDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn sample() -> Geometry {
        Geometry::new(vec![
            Autd3::default(),
            Autd3::new(
                Point3::new(192.0, -10.5, 300.0),
                UnitQuaternion::from_euler_angles(0.0, 0.5, 0.0),
            ),
        ])
    }

    #[test]
    fn a_saved_geometry_loads_back() {
        let scratch = ScratchDir::new("round-trip");
        let path = scratch.file();
        save(&path, &sample()).unwrap();
        assert!(!staging_path(&path).exists());
        let restored = load(&path).unwrap();
        assert_eq!(device_poses(&restored), device_poses(&sample()));
    }

    #[test]
    fn saving_again_replaces_the_file() {
        let scratch = ScratchDir::new("replace");
        let path = scratch.file();
        save(&path, &sample()).unwrap();
        save(&path, &Geometry::new(vec![Autd3::default()])).unwrap();
        assert_eq!(load(&path).unwrap().num_devices(), 1);
    }

    #[test]
    fn a_missing_file_starts_with_one_device_at_the_origin() {
        let scratch = ScratchDir::new("missing");
        let geometry = load_or_default(Some(&scratch.file()));
        assert_eq!(
            device_poses(&geometry),
            device_poses(&Geometry::new(vec![Autd3::default()]))
        );
    }

    #[test]
    fn a_broken_file_starts_with_one_device_at_the_origin() {
        let scratch = ScratchDir::new("broken");
        let path = scratch.file();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        for content in [
            "",
            "{",
            "[]",
            r#"[{"origin":[0,0,0],"rotation":[2,0,0,0]}]"#,
        ] {
            fs::write(&path, content).unwrap();
            assert!(load(&path).is_err());
            assert_eq!(load_or_default(Some(&path)).num_devices(), 1);
        }
    }

    #[test]
    fn no_path_starts_with_one_device_at_the_origin() {
        assert_eq!(load_or_default(None).num_devices(), 1);
    }
}
