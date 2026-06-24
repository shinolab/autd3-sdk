use std::path::{Path, PathBuf};

const FW_SOURCES: &[&str] = &[
    "app.c",
    "proto.c",
    "fpga.c",
    "cmd/xor_hash.c",
    "cmd/write_pattern.c",
    "cmd/write_pattern_compressed.c",
    "cmd/write_mod.c",
    "cmd/config_mod.c",
    "cmd/config_pattern.c",
    "cmd/change_mod_bank.c",
    "cmd/change_pattern_bank.c",
    "cmd/clear.c",
    "cmd/sync.c",
    "cmd/set_mode.c",
    "cmd/silencer.c",
];

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    let vendored = manifest.join("vendor/cpu");
    let fw = if vendored.join("inc").is_dir() {
        vendored
    } else {
        let sibling = manifest.join("../../firmware/cpu");
        assert!(
            sibling.join("inc").is_dir(),
            "firmware sources not found: expected a vendored copy at {} or the in-repo \
             firmware at {}. Run `cargo xtask vendor-fw` before packaging.",
            manifest.join("vendor/cpu").display(),
            sibling.display(),
        );
        sibling
    };
    let fw_inc = fw.join("inc");
    let fw_src = fw.join("src");
    let csrc = manifest.join("csrc");

    let mut build = cc::Build::new();
    build
        .std("c11")
        .include(&fw_inc)
        .include(&fw_src)
        .include(&csrc);
    for s in FW_SOURCES {
        build.file(fw_src.join(s));
    }
    build.file(csrc.join("emu_glue.c"));
    build.compile("autd3_fw_emu");

    let bindings = bindgen::Builder::default()
        .header(csrc.join("wrapper.h").to_str().unwrap())
        .clang_arg(format!("-I{}", fw_inc.display()))
        .clang_arg(format!("-I{}", fw_src.display()))
        .clang_arg(format!("-I{}", csrc.display()))
        .blocklist_function("port_.*")
        .use_core()
        .layout_tests(false)
        .generate()
        .expect("failed to generate firmware bindings");
    bindings
        .write_to_file(out.join("bindings.rs"))
        .expect("failed to write firmware bindings");

    rerun_if_changed(&csrc);
    rerun_if_changed(&fw_inc);
    rerun_if_changed(&fw_src);
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
