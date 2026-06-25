#[cfg(not(target_os = "windows"))]
fn main() {}

#[cfg(target_os = "windows")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::env;
    use std::path::Path;

    let manifest_dir = env::var("CARGO_MANIFEST_DIR")?;
    let out_dir = env::var("OUT_DIR")?;
    let target = env::var("TARGET")?;

    let machine = if target.starts_with("aarch64") {
        "ARM64"
    } else {
        "X64"
    };

    let tool = cc::windows_registry::find_tool(&target, "lib.exe")
        .ok_or("lib.exe not found. Please install the MSVC Build Tools.")?;

    for name in ["wpcap", "Packet"] {
        let def = Path::new(&manifest_dir)
            .join("def")
            .join(format!("{name}.def"));
        let lib = Path::new(&out_dir).join(format!("{name}.lib"));

        println!("cargo:rerun-if-changed={}", def.display());

        let status = tool
            .to_command()
            .arg(format!("/MACHINE:{machine}"))
            .arg(format!("/DEF:{}", def.display()))
            .arg(format!("/OUT:{}", lib.display()))
            .status()?;
        if !status.success() {
            return Err(format!("lib.exe failed to generate {}", lib.display()).into());
        }
    }

    println!("cargo:rustc-link-search=native={out_dir}");

    Ok(())
}
