use std::path::Path;

fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let out = std::env::var("OUT_DIR").unwrap();
    let manifest = Path::new(&manifest);
    let out = Path::new(&out);

    let license = std::fs::read_to_string(manifest.join("../../LICENSE"))
        .unwrap_or_else(|_| "MIT License — see https://github.com/shinolab/autd3-sdk".to_string());
    std::fs::write(out.join("license.txt"), license).unwrap();

    let third_party = std::fs::read_to_string(manifest.join("../THIRD-PARTY-LICENSES.md"))
        .unwrap_or_else(|_| {
            "Third-party license information is generated at packaging time \
             (`cargo xtask simulator build` or `cargo xtask license generate`)."
                .to_string()
        });
    std::fs::write(out.join("third-party.md"), third_party).unwrap();

    println!("cargo:rerun-if-changed=../THIRD-PARTY-LICENSES.md");
    println!("cargo:rerun-if-changed=../../LICENSE");
}
