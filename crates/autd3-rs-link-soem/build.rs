use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=3rdparty/SOEM");

    let dst = cmake::Config::new("3rdparty/SOEM")
        .define("SOEM_BUILD_SAMPLES", "OFF")
        .build();
    println!(
        "cargo:rustc-link-search=native={}",
        dst.join("lib").display()
    );
    println!("cargo:rustc-link-lib=static=soem");

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
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

    generate_bindings(&dst, &manifest_dir);
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
