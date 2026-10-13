use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let script = manifest_dir.join("../platform/autd3-cpu-loader.ld");
    println!("cargo:rerun-if-changed={}", script.display());
    println!(
        "cargo:rustc-link-arg-bin=autd3-cpu-loader=-T{}",
        script.display()
    );
}
