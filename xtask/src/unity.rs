use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use flate2::read::GzDecoder;
use tar::Archive;

use crate::util::{on_path, run, run_tool};

const SOEM_CRATE: &str = "autd3-ffi-link-soem";

pub const PKG_PREFIX: &str = "com.shinolab.autd3-sdk";

const RIDS: &[&str] = &["win-x64", "linux-x64", "osx-arm64"];

const CLIENT_PKG: &str = "com.shinolab.autd3-sdk";
const CLIENT_SAMPLE: &str = "Samples~/FocusSine";

const DOC_FILES: &[&str] = &[
    "README.md",
    "CHANGELOG.md",
    "LICENSE.md",
    "THIRD-PARTY-LICENSES.md",
    "COPYING",
    "NOTICE",
];

struct UnityPkg {
    id: &'static str,
    assembly: &'static str,
    lib: &'static str,
    gpl: bool,
}

const PACKAGES: &[UnityPkg] = &[
    UnityPkg {
        id: "com.shinolab.autd3-sdk.core",
        assembly: "AUTD3.Core",
        lib: "autd3_core",
        gpl: false,
    },
    UnityPkg {
        id: "com.shinolab.autd3-sdk",
        assembly: "AUTD3",
        lib: "autd3capi",
        gpl: false,
    },
    UnityPkg {
        id: "com.shinolab.autd3-sdk.pattern",
        assembly: "AUTD3.Pattern",
        lib: "autd3_pattern",
        gpl: false,
    },
    UnityPkg {
        id: "com.shinolab.autd3-sdk.pattern.holo",
        assembly: "AUTD3.Pattern.Holo",
        lib: "autd3_pattern_holo",
        gpl: false,
    },
    UnityPkg {
        id: "com.shinolab.autd3-sdk.modulation",
        assembly: "AUTD3.Modulation",
        lib: "autd3_modulation",
        gpl: false,
    },
    UnityPkg {
        id: "com.shinolab.autd3-sdk.link.echocat",
        assembly: "AUTD3.Link.Echocat",
        lib: "autd3_link_echocat",
        gpl: false,
    },
    UnityPkg {
        id: "com.shinolab.autd3-sdk.link.ethercrab",
        assembly: "AUTD3.Link.Ethercrab",
        lib: "autd3_link_ethercrab",
        gpl: false,
    },
    UnityPkg {
        id: "com.shinolab.autd3-sdk.link.nop",
        assembly: "AUTD3.Link.Nop",
        lib: "autd3_link_nop",
        gpl: false,
    },
    UnityPkg {
        id: "com.shinolab.autd3-sdk.link.remote",
        assembly: "AUTD3.Link.Remote",
        lib: "autd3_link_remote",
        gpl: false,
    },
    UnityPkg {
        id: "com.shinolab.autd3-sdk.link.twincat",
        assembly: "AUTD3.Link.TwinCAT",
        lib: "autd3_link_twincat",
        gpl: false,
    },
    UnityPkg {
        id: "com.shinolab.autd3-sdk.link.soem",
        assembly: "AUTD3.Link.Soem",
        lib: "autd3_link_soem",
        gpl: true,
    },
];

#[derive(Subcommand)]
pub enum UnityCmd {
    /// Stage the C# sources and the host-RID native libs into the UPM packages
    Build {
        /// Also build the SOEM native lib (opt-in: it is GPL-3.0-only)
        #[arg(long)]
        soem: bool,
        /// Also write a `Packages/manifest.json` snippet for the staged packages
        #[arg(long)]
        manifest: bool,
    },
    /// Pack the UPM packages into `bindings/unity/dist/` and verify the tarballs
    Pack {
        /// Directory holding the per-RID native libraries; the host RID only if omitted
        #[arg(long)]
        native_dir: Option<PathBuf>,
        /// Directory to write the `.tgz` files to
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// Run the Unity Test Framework tests
    Test {
        /// Path to the Unity Editor executable
        #[arg(long)]
        unity_editor: Option<PathBuf>,
    },
}

pub fn run_unity(root: &Path, cmd: UnityCmd) -> Result<()> {
    match cmd {
        UnityCmd::Build { soem, manifest } => build(root, soem, manifest),
        UnityCmd::Pack { native_dir, out } => pack(root, native_dir.as_deref(), out),
        UnityCmd::Test { unity_editor } => test(root, unity_editor),
    }
}

fn build(root: &Path, soem: bool, manifest: bool) -> Result<()> {
    let ffi = root.join("bindings").join("ffi");
    let unity_dir = root.join("bindings").join("unity");

    if soem {
        run("cargo", ["build", "--workspace", "--release"], &ffi)?;
    } else {
        run(
            "cargo",
            ["build", "--workspace", "--exclude", SOEM_CRATE, "--release"],
            &ffi,
        )?;
    }
    let native = ffi.join("target").join("release");
    let rid = host_rid()?;

    for pkg in PACKAGES {
        let pkg_dir = unity_dir.join(pkg.id);
        stage_package(root, pkg, &pkg_dir)?;
        stage_native(&native, rid, pkg, &pkg_dir, true)?;
    }

    println!(
        "unity build: assembled {} packages under {}",
        PACKAGES.len(),
        unity_dir.display()
    );
    if manifest {
        emit_manifest(&unity_dir);
    }
    Ok(())
}

fn pack(root: &Path, native_dir: Option<&Path>, out: Option<PathBuf>) -> Result<()> {
    if !on_path("npm") {
        bail!("`npm` is required for `unity pack` (install Node.js)");
    }
    let ffi = root.join("bindings").join("ffi");
    let unity_dir = root.join("bindings").join("unity");
    let out_dir = out.unwrap_or_else(|| unity_dir.join("dist"));

    let version = verify_versions(root, &unity_dir)?;

    let rids: Vec<&str> = if let Some(dir) = native_dir {
        let missing: Vec<&str> = RIDS
            .iter()
            .copied()
            .filter(|rid| !dir.join(rid).is_dir())
            .collect();
        if !missing.is_empty() {
            bail!(
                "native dir {} is missing rid(s): {}",
                dir.display(),
                missing.join(", ")
            );
        }
        RIDS.to_vec()
    } else {
        run("cargo", ["build", "--workspace", "--release"], &ffi)?;
        vec![host_rid()?]
    };

    crate::license::generate_unity(root)?;

    if out_dir.exists() {
        std::fs::remove_dir_all(&out_dir)?;
    }
    std::fs::create_dir_all(&out_dir)?;

    for pkg in PACKAGES {
        let pkg_dir = unity_dir.join(pkg.id);
        stage_package(root, pkg, &pkg_dir)?;
        for rid in &rids {
            let native =
                native_dir.map_or_else(|| ffi.join("target").join("release"), |dir| dir.join(rid));
            stage_native(&native, rid, pkg, &pkg_dir, false)?;
        }
        npm_pack(&pkg_dir, &out_dir)?;
        let tarball = out_dir.join(format!("{}-{version}.tgz", pkg.id));
        verify_tarball(&tarball, pkg, &rids)?;
    }

    println!(
        "unity pack: wrote {} tarballs ({}) to {}",
        PACKAGES.len(),
        rids.join(", "),
        out_dir.display()
    );
    Ok(())
}

fn stage_package(root: &Path, pkg: &UnityPkg, pkg_dir: &Path) -> Result<()> {
    let csharp_src = root.join("bindings").join("csharp").join("src");
    let package_json = pkg_dir.join("package.json");
    let asmdef = pkg_dir.join(format!("{}.asmdef", pkg.assembly));
    let csc_rsp = pkg_dir.join("csc.rsp");
    let readme = pkg_dir.join("README.md");
    for required in [&package_json, &asmdef, &csc_rsp, &readme] {
        if !required.is_file() {
            bail!(
                "missing committed package file for {}: {}",
                pkg.id,
                required.display()
            );
        }
    }

    clean_generated(pkg_dir)?;

    let src_dir = csharp_src.join(pkg.assembly);
    stage_sources(&src_dir, pkg_dir)?;
    write_meta(
        &pkg_dir.join(format!("{}.asmdef.meta", pkg.assembly)),
        &asmdef_meta(&guid_for(pkg.id, &format!("{}.asmdef", pkg.assembly))),
    )?;
    write_meta(
        &pkg_dir.join("csc.rsp.meta"),
        &default_meta(&guid_for(pkg.id, "csc.rsp")),
    )?;
    write_meta(
        &pkg_dir.join("package.json.meta"),
        &package_manifest_meta(&guid_for(pkg.id, "package.json")),
    )?;
    for doc in DOC_FILES {
        if pkg_dir.join(doc).is_file() {
            write_meta(
                &pkg_dir.join(format!("{doc}.meta")),
                &default_meta(&guid_for(pkg.id, doc)),
            )?;
        }
    }
    Ok(())
}

fn npm_pack(pkg_dir: &Path, out_dir: &Path) -> Result<()> {
    run_tool(
        "npm",
        [
            "pack",
            "--silent",
            "--pack-destination",
            &out_dir.to_string_lossy(),
        ],
        pkg_dir,
    )
}

fn clean_generated(pkg_dir: &Path) -> Result<()> {
    let doc_metas: Vec<String> = DOC_FILES.iter().map(|f| format!("{f}.meta")).collect();
    for entry in std::fs::read_dir(pkg_dir)
        .with_context(|| format!("reading {}", pkg_dir.display()))?
        .flatten()
    {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_file()
            && (name.ends_with(".cs")
                || name.ends_with(".cs.meta")
                || name.ends_with(".asmdef.meta")
                || name == "csc.rsp.meta"
                || name == "package.json.meta"
                || doc_metas.iter().any(|m| *m == name))
        {
            std::fs::remove_file(&path)?;
        }
    }
    let plugins = pkg_dir.join("Plugins");
    if plugins.exists() {
        std::fs::remove_dir_all(&plugins)?;
    }
    let plugins_meta = pkg_dir.join("Plugins.meta");
    if plugins_meta.exists() {
        std::fs::remove_file(&plugins_meta)?;
    }
    Ok(())
}

fn stage_sources(src_dir: &Path, pkg_dir: &Path) -> Result<()> {
    let pkg_id = pkg_dir.file_name().unwrap().to_string_lossy().into_owned();
    for entry in std::fs::read_dir(src_dir)
        .with_context(|| format!("reading source dir {}", src_dir.display()))?
        .flatten()
    {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("cs") {
            continue;
        }
        let file_name = entry.file_name();
        let dst = pkg_dir.join(&file_name);
        std::fs::copy(&path, &dst)
            .with_context(|| format!("staging {} -> {}", path.display(), dst.display()))?;
        let rel = file_name.to_string_lossy();
        write_meta(
            &pkg_dir.join(format!("{rel}.meta")),
            &cs_meta(&guid_for(&pkg_id, &rel)),
        )?;
    }
    Ok(())
}

fn stage_native(
    native: &Path,
    rid: &str,
    pkg: &UnityPkg,
    pkg_dir: &Path,
    allow_missing_gpl: bool,
) -> Result<()> {
    let (prefix, ext) = rid_affix(rid);
    let file = format!("{prefix}{}.{ext}", pkg.lib);
    let src = native.join(&file);
    if !src.is_file() {
        if pkg.gpl && allow_missing_gpl {
            println!(
                "unity build: skipping {} native lib (GPL, build with --soem to include)",
                pkg.id
            );
            return Ok(());
        }
        bail!("native lib not found: {}", src.display());
    }

    let plugins = pkg_dir.join("Plugins");
    let rid_dir = plugins.join(rid);
    std::fs::create_dir_all(&rid_dir)?;
    let dst = rid_dir.join(&file);
    std::fs::copy(&src, &dst)
        .with_context(|| format!("staging {} -> {}", src.display(), dst.display()))?;

    write_meta(
        &pkg_dir.join("Plugins.meta"),
        &folder_meta(&guid_for(pkg.id, "Plugins")),
    )?;
    write_meta(
        &plugins.join(format!("{rid}.meta")),
        &folder_meta(&guid_for(pkg.id, &format!("Plugins/{rid}"))),
    )?;
    write_meta(
        &rid_dir.join(format!("{file}.meta")),
        &plugin_meta(&guid_for(pkg.id, &format!("Plugins/{rid}/{file}")), rid),
    )?;
    Ok(())
}

fn write_meta(path: &Path, content: &str) -> Result<()> {
    std::fs::write(path, content).with_context(|| format!("writing {}", path.display()))
}

fn verify_versions(root: &Path, unity_dir: &Path) -> Result<String> {
    let mut version: Option<String> = None;
    for pkg in PACKAGES {
        let path = unity_dir.join(pkg.id).join("package.json");
        let found = package_json_version(&path)?;
        match &version {
            None => version = Some(found),
            Some(first) if *first != found => bail!(
                "unity package versions diverge: {} is {found}, expected {first}",
                pkg.id
            ),
            Some(_) => {}
        }
    }
    let version = version.context("no unity packages")?;

    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() != 3
        || !parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
    {
        bail!(
            "unity version `{version}` must be major.minor.patch; npm rejects a 4th component, \
             strips `+build` metadata, and orders `-1` prereleases below the plain release"
        );
    }

    let cargo = workspace_version(&root.join("Cargo.toml"))?;
    let cargo_minor: Vec<&str> = cargo.split('.').take(2).collect();
    if parts[..2] != cargo_minor[..] {
        bail!(
            "unity version `{version}` must share major.minor with the Cargo workspace version `{cargo}`"
        );
    }

    for pkg in PACKAGES {
        let path = unity_dir.join(pkg.id).join("package.json");
        for (dep, req) in package_json_deps(&path)? {
            if req != version {
                bail!(
                    "{}: dependency `{dep}` is pinned to `{req}`, expected `{version}`; npm rejects a package whose sibling dependency is not published",
                    pkg.id
                );
            }
        }
    }
    Ok(version)
}

fn package_json_deps(path: &Path) -> Result<Vec<(String, String)>> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let prefix = format!("\"{PKG_PREFIX}");
    let mut deps = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with(&prefix) {
            continue;
        }
        let (name, req) = trimmed
            .split_once(':')
            .with_context(|| format!("malformed dependency line in {}: {line}", path.display()))?;
        deps.push((
            name.trim().trim_matches('"').to_owned(),
            req.trim()
                .trim_end_matches(',')
                .trim_matches('"')
                .to_owned(),
        ));
    }
    Ok(deps)
}

fn package_json_version(path: &Path) -> Result<String> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("\"version\":") {
            let value = rest
                .trim()
                .trim_end_matches(',')
                .trim_matches('"')
                .to_string();
            return Ok(value);
        }
    }
    bail!("no \"version\" field in {}", path.display())
}

fn workspace_version(cargo_toml: &Path) -> Result<String> {
    let text = std::fs::read_to_string(cargo_toml)
        .with_context(|| format!("reading {}", cargo_toml.display()))?;
    let doc: toml_edit::DocumentMut = text
        .parse()
        .with_context(|| format!("parsing {}", cargo_toml.display()))?;
    doc["workspace"]["package"]["version"]
        .as_str()
        .map(str::to_string)
        .context("no [workspace.package] version")
}

fn verify_tarball(tarball: &Path, pkg: &UnityPkg, rids: &[&str]) -> Result<()> {
    let file =
        std::fs::File::open(tarball).with_context(|| format!("opening {}", tarball.display()))?;
    let mut archive = Archive::new(GzDecoder::new(file));
    let mut entries = Vec::new();
    for entry in archive
        .entries()
        .with_context(|| format!("reading {}", tarball.display()))?
    {
        let entry = entry?;
        let path = entry.path()?.to_string_lossy().into_owned();
        let Some(rel) = path.strip_prefix("package/") else {
            bail!("{}: entry outside package/ root: {path}", tarball.display());
        };
        entries.push(rel.to_string());
    }

    let has = |name: &str| entries.iter().any(|e| e == name);
    let mut required = vec![
        "package.json".to_string(),
        "package.json.meta".to_string(),
        format!("{}.asmdef", pkg.assembly),
        format!("{}.asmdef.meta", pkg.assembly),
        "csc.rsp".to_string(),
        "csc.rsp.meta".to_string(),
        "README.md".to_string(),
        "LICENSE.md".to_string(),
        "THIRD-PARTY-LICENSES.md".to_string(),
    ];
    for rid in rids {
        let (prefix, ext) = rid_affix(rid);
        required.push(format!("Plugins/{rid}/{prefix}{}.{ext}", pkg.lib));
        required.push(format!("Plugins/{rid}/{prefix}{}.{ext}.meta", pkg.lib));
    }
    for name in &required {
        if !has(name) {
            bail!("{}: missing {name}", tarball.display());
        }
    }

    let sources: Vec<&String> = entries
        .iter()
        .filter(|e| !e.starts_with("Samples~/"))
        .filter(|e| Path::new(e).extension().is_some_and(|ext| ext == "cs"))
        .collect();
    if sources.is_empty() {
        bail!("{}: no .cs sources", tarball.display());
    }
    for src in &sources {
        let meta = format!("{src}.meta");
        if !has(&meta) {
            bail!("{}: {src} has no .meta", tarball.display());
        }
    }

    let sample = entries.iter().any(|e| e.starts_with(CLIENT_SAMPLE));
    if pkg.id == CLIENT_PKG && !sample {
        bail!("{}: missing {CLIENT_SAMPLE}", tarball.display());
    }
    if pkg.id != CLIENT_PKG && sample {
        bail!("{}: unexpected {CLIENT_SAMPLE}", tarball.display());
    }
    Ok(())
}

fn emit_manifest(unity_dir: &Path) {
    println!("\n// add these to your Unity project's Packages/manifest.json \"dependencies\":");
    for pkg in PACKAGES {
        let path = unity_dir.join(pkg.id);
        println!("  \"{}\": \"file:{}\",", pkg.id, path.display());
    }
}

fn test(root: &Path, unity_editor: Option<PathBuf>) -> Result<()> {
    let managed = resolve_managed_dir(unity_editor)?;
    let core_module = managed.join("UnityEngine.CoreModule.dll");
    if !core_module.is_file() {
        bail!(
            "UnityEngine.CoreModule.dll not found under {} (pass --unity-editor <editor-root> or set AUTD3_UNITY_EDITOR)",
            managed.display()
        );
    }
    let proj = root
        .join("bindings")
        .join("csharp")
        .join("tests")
        .join("AUTD3.Unity.Tests")
        .join("AUTD3.Unity.Tests.csproj");
    let cwd = proj.parent().unwrap();
    let mut cmd = Command::new("dotnet");
    cmd.args([
        "test",
        &proj.to_string_lossy(),
        "-c",
        "Debug",
        &format!("-p:UnityManagedDir={}", managed.display()),
    ])
    .current_dir(cwd);
    let status = cmd
        .status()
        .with_context(|| "failed to spawn `dotnet`".to_string())?;
    if !status.success() {
        bail!("`dotnet test` exited with {status}");
    }
    Ok(())
}

fn resolve_managed_dir(unity_editor: Option<PathBuf>) -> Result<PathBuf> {
    let editor = unity_editor
        .or_else(|| std::env::var_os("AUTD3_UNITY_EDITOR").map(PathBuf::from))
        .or_else(|| std::env::var_os("UNITY_EDITOR").map(PathBuf::from))
        .context(
            "Unity Editor path required: pass --unity-editor <editor-root> or set AUTD3_UNITY_EDITOR",
        )?;
    for candidate in [
        editor.join("Data").join("Managed").join("UnityEngine"),
        editor.join("Contents").join("Managed").join("UnityEngine"),
        editor.clone(),
    ] {
        if candidate.join("UnityEngine.CoreModule.dll").is_file() {
            return Ok(candidate);
        }
    }
    Ok(editor.join("Data").join("Managed").join("UnityEngine"))
}

fn host_rid() -> Result<&'static str> {
    Ok(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "linux-x64",
        ("windows", "x86_64") => "win-x64",
        ("macos", "aarch64") => "osx-arm64",
        ("macos", "x86_64") => "osx-x64",
        (os, arch) => bail!("unsupported host {os}/{arch} for `unity build`"),
    })
}

fn rid_affix(rid: &str) -> (&'static str, &'static str) {
    if rid.starts_with("win") {
        ("", "dll")
    } else if rid.starts_with("osx") {
        ("lib", "dylib")
    } else {
        ("lib", "so")
    }
}

fn guid_for(pkg_id: &str, rel: &str) -> String {
    let input = format!("{pkg_id}/{rel}");
    let a = fnv1a(input.as_bytes(), 0xcbf2_9ce4_8422_2325);
    let b = fnv1a(input.as_bytes(), 0x8422_2325_cbf2_9ce4);
    format!("{a:016x}{b:016x}")
}

fn fnv1a(bytes: &[u8], mut hash: u64) -> u64 {
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn cs_meta(guid: &str) -> String {
    format!(
        "fileFormatVersion: 2\nguid: {guid}\nMonoImporter:\n  externalObjects: {{}}\n  serializedVersion: 2\n  defaultReferences: []\n  executionOrder: 0\n  icon: {{instanceID: 0}}\n  userData: \n  assetBundleName: \n  assetBundleVariant: \n"
    )
}

fn asmdef_meta(guid: &str) -> String {
    format!(
        "fileFormatVersion: 2\nguid: {guid}\nAssemblyDefinitionImporter:\n  externalObjects: {{}}\n  userData: \n  assetBundleName: \n  assetBundleVariant: \n"
    )
}

fn default_meta(guid: &str) -> String {
    format!(
        "fileFormatVersion: 2\nguid: {guid}\nDefaultImporter:\n  externalObjects: {{}}\n  userData: \n  assetBundleName: \n  assetBundleVariant: \n"
    )
}

fn package_manifest_meta(guid: &str) -> String {
    format!(
        "fileFormatVersion: 2\nguid: {guid}\nPackageManifestImporter:\n  externalObjects: {{}}\n  userData: \n  assetBundleName: \n  assetBundleVariant: \n"
    )
}

fn folder_meta(guid: &str) -> String {
    format!(
        "fileFormatVersion: 2\nguid: {guid}\nfolderAsset: yes\nDefaultImporter:\n  externalObjects: {{}}\n  userData: \n  assetBundleName: \n  assetBundleVariant: \n"
    )
}

fn plugin_meta(guid: &str, rid: &str) -> String {
    let (cpu, os, standalone) = match rid {
        "win-x64" => ("x86_64", "Windows", "Win64"),
        "osx-arm64" => ("ARM64", "OSX", "OSXUniversal"),
        "osx-x64" => ("x86_64", "OSX", "OSXUniversal"),
        _ => ("x86_64", "Linux", "Linux64"),
    };
    format!(
        r"fileFormatVersion: 2
guid: {guid}
PluginImporter:
  externalObjects: {{}}
  serializedVersion: 2
  iconMap: {{}}
  executionOrder: {{}}
  defineConstraints: []
  isPreloaded: 0
  isOverridable: 0
  isExplicitlyReferenced: 0
  validateReferences: 1
  platformData:
  - first:
      Any:
    second:
      enabled: 0
      settings: {{}}
  - first:
      Editor: Editor
    second:
      enabled: 1
      settings:
        CPU: {cpu}
        DefaultValueInitialized: true
        OS: {os}
  - first:
      Standalone: {standalone}
    second:
      enabled: 1
      settings:
        CPU: {cpu}
  userData:
  assetBundleName:
  assetBundleVariant:
"
    )
}
