use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=3rdparty/SOEM");

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let soem_src = prepare_soem_src(&manifest_dir);

    let dst = cmake::Config::new(&soem_src)
        .define("SOEM_BUILD_SAMPLES", "OFF")
        .build();
    println!(
        "cargo:rustc-link-search=native={}",
        dst.join("lib").display()
    );
    println!("cargo:rustc-link-lib=static=soem");

    #[cfg(target_os = "windows")]
    {
        println!("cargo:rustc-link-lib=winmm");
        println!("cargo:rustc-link-lib=ws2_32");
        println!(
            "cargo:rustc-link-search=native={}",
            manifest_dir
                .join("3rdparty/SOEM/oshw/win32/wpcap/Lib/x64")
                .display()
        );
        println!("cargo:rustc-link-lib=wpcap");
        println!("cargo:rustc-link-lib=Packet");
    }
    #[cfg(target_os = "linux")]
    {
        println!("cargo:rustc-link-lib=pthread");
        println!("cargo:rustc-link-lib=rt");
    }
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-lib=pcap");
    }

    generate_bindings(&dst, &manifest_dir);
}

#[cfg(not(target_os = "macos"))]
fn prepare_soem_src(manifest_dir: &Path) -> PathBuf {
    manifest_dir.join("3rdparty/SOEM")
}

#[cfg(target_os = "macos")]
fn prepare_soem_src(manifest_dir: &Path) -> PathBuf {
    println!("cargo:rerun-if-changed=macos");

    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let src = manifest_dir.join("3rdparty/SOEM");
    let staged = out.join("soem-src");
    if staged.exists() {
        std::fs::remove_dir_all(&staged).expect("failed to clear SOEM staging dir");
    }
    copy_dir(&src, &staged);

    let osal = staged.join("osal/macosx");
    let oshw = staged.join("oshw/macosx");
    std::fs::create_dir_all(&osal).expect("failed to create osal/macosx");
    std::fs::create_dir_all(&oshw).expect("failed to create oshw/macosx");

    for (from, to) in [
        (staged.join("oshw/linux/oshw.c"), oshw.join("oshw.c")),
        (staged.join("oshw/linux/oshw.h"), oshw.join("oshw.h")),
        (
            staged.join("osal/linux/osal_defs.h"),
            osal.join("osal_defs.h"),
        ),
    ] {
        std::fs::copy(&from, &to)
            .unwrap_or_else(|e| panic!("failed to stage {}: {e}", from.display()));
    }

    let macos = manifest_dir.join("macos");
    for (file, dir) in [("osal.c", &osal), ("nicdrv.c", &oshw), ("nicdrv.h", &oshw)] {
        std::fs::copy(macos.join(file), dir.join(file))
            .unwrap_or_else(|e| panic!("failed to stage macos/{file}: {e}"));
    }
    std::fs::copy(
        macos.join("Darwin.cmake"),
        staged.join("cmake/Darwin.cmake"),
    )
    .expect("failed to stage Darwin.cmake");

    staged
}

#[cfg(target_os = "macos")]
fn copy_dir(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).expect("failed to create staging dir");
    for entry in std::fs::read_dir(src).expect("failed to read SOEM tree") {
        let entry = entry.expect("failed to read SOEM dir entry");
        let ty = entry.file_type().expect("failed to stat SOEM dir entry");
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir(&from, &to);
        } else if ty.is_file() {
            std::fs::copy(&from, &to).expect("failed to copy SOEM file");
        }
    }
}

fn generate_bindings(dst: &std::path::Path, manifest_dir: &std::path::Path) {
    let include = dst.join("include");
    let include_soem = include.join("soem");
    let header = include_soem.join("soem.h");

    let mut builder = bindgen::Builder::default()
        .header(header.to_string_lossy())
        .clang_arg(format!("-I{}", include.display()))
        .clang_arg(format!("-I{}", include_soem.display()))
        .allowlist_function("ec_.*")
        .allowlist_function("ecx_.*")
        .allowlist_type("ec_.*")
        .allowlist_type("ecx_.*")
        .allowlist_type("ECT_.*")
        .allowlist_var("EC_.*")
        .allowlist_var("ECT_.*")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));

    if cfg!(target_os = "windows") {
        builder = builder.clang_arg(format!(
            "-I{}",
            manifest_dir
                .join("3rdparty/SOEM/oshw/win32/wpcap/Include")
                .display()
        ));
    }

    let out_path = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    builder
        .generate()
        .expect("failed to generate SOEM bindings")
        .write_to_file(out_path.join("bindings.rs"))
        .expect("failed to write SOEM bindings");
}
