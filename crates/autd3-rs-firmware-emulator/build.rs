use std::fmt::Write;
use std::path::{Path, PathBuf};

// The firmware modules are compiled *into* this crate rather than depended on:
// `autd3-cpu-fw` is not published to crates.io, so a path dependency would make
// this crate unpublishable. In-repo builds always use the canonical
// `firmware/cpu/fw/src`; `cargo xtask vendor-fw` copies it into `vendor/cpu-fw/`
// only as the packaging fallback so the published crate is self-contained.
const FW_MODULES: &[(&str, &str)] = &[
    ("params", "params.rs"),
    ("version", "version.rs"),
    ("proto", "proto.rs"),
    ("port", "port.rs"),
    ("fpga", "fpga.rs"),
    ("cmd", "cmd/mod.rs"),
    ("app", "app.rs"),
];

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    let vendored = manifest.join("vendor/cpu-fw");
    let in_repo = manifest.join("../../firmware/cpu/fw/src");
    let fw = if in_repo.join("lib.rs").is_file() {
        in_repo
    } else {
        assert!(
            vendored.join("lib.rs").is_file(),
            "firmware sources not found: expected the in-repo firmware at {} or a vendored \
             copy at {}. Run `cargo xtask vendor-fw` before packaging.",
            in_repo.display(),
            vendored.display(),
        );
        vendored
    };

    let mut root = String::new();
    for (name, file) in FW_MODULES {
        let path = fw.join(file);
        assert!(path.is_file(), "missing firmware source {}", path.display());
        writeln!(
            root,
            "#[allow(dead_code, clippy::all, clippy::pedantic)]\n\
             #[path = {:?}]\n\
             pub(crate) mod {name};",
            path.to_string_lossy(),
        )
        .unwrap();
    }
    std::fs::write(out.join("fw_root.rs"), root).expect("failed to write fw_root.rs");

    rerun_if_changed(&fw);
}

fn rerun_if_changed(dir: &Path) {
    println!("cargo:rerun-if-changed={}", dir.display());
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rerun_if_changed(&path);
        } else {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
}
