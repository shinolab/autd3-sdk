use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::Subcommand;

use crate::util::run;

const SOEM_CRATE: &str = "autd3-ffi-link-soem";

struct UnityPkg {
    id: &'static str,
    assembly: &'static str,
    lib: &'static str,
    gpl: bool,
}

const PACKAGES: &[UnityPkg] = &[
    UnityPkg {
        id: "com.shinolab.autd3.core",
        assembly: "AUTD3.Core",
        lib: "autd3_core",
        gpl: false,
    },
    UnityPkg {
        id: "com.shinolab.autd3",
        assembly: "AUTD3",
        lib: "autd3capi",
        gpl: false,
    },
    UnityPkg {
        id: "com.shinolab.autd3.pattern",
        assembly: "AUTD3.Pattern",
        lib: "autd3_pattern",
        gpl: false,
    },
    UnityPkg {
        id: "com.shinolab.autd3.pattern.holo",
        assembly: "AUTD3.Pattern.Holo",
        lib: "autd3_pattern_holo",
        gpl: false,
    },
    UnityPkg {
        id: "com.shinolab.autd3.modulation",
        assembly: "AUTD3.Modulation",
        lib: "autd3_modulation",
        gpl: false,
    },
    UnityPkg {
        id: "com.shinolab.autd3.link.ethercrab",
        assembly: "AUTD3.Link.Ethercrab",
        lib: "autd3_link_ethercrab",
        gpl: false,
    },
    UnityPkg {
        id: "com.shinolab.autd3.link.nop",
        assembly: "AUTD3.Link.Nop",
        lib: "autd3_link_nop",
        gpl: false,
    },
    UnityPkg {
        id: "com.shinolab.autd3.link.remote",
        assembly: "AUTD3.Link.Remote",
        lib: "autd3_link_remote",
        gpl: false,
    },
    UnityPkg {
        id: "com.shinolab.autd3.link.twincat",
        assembly: "AUTD3.Link.TwinCAT",
        lib: "autd3_link_twincat",
        gpl: false,
    },
    UnityPkg {
        id: "com.shinolab.autd3.link.soem",
        assembly: "AUTD3.Link.Soem",
        lib: "autd3_link_soem",
        gpl: true,
    },
];

#[derive(Subcommand)]
pub enum UnityCmd {
    Build {
        #[arg(long)]
        soem: bool,
        #[arg(long)]
        manifest: bool,
    },
    Test {
        #[arg(long)]
        unity_editor: Option<PathBuf>,
    },
}

pub fn run_unity(root: &Path, cmd: UnityCmd) -> Result<()> {
    match cmd {
        UnityCmd::Build { soem, manifest } => build(root, soem, manifest),
        UnityCmd::Test { unity_editor } => test(root, unity_editor),
    }
}

fn build(root: &Path, soem: bool, manifest: bool) -> Result<()> {
    let ffi = root.join("bindings").join("ffi");
    let unity_dir = root.join("bindings").join("unity");
    let csharp_src = root.join("bindings").join("csharp").join("src");

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
        let package_json = pkg_dir.join("package.json");
        let asmdef = pkg_dir.join(format!("{}.asmdef", pkg.assembly));
        let csc_rsp = pkg_dir.join("csc.rsp");
        for required in [&package_json, &asmdef, &csc_rsp] {
            if !required.is_file() {
                bail!(
                    "missing committed package file for {}: {}",
                    pkg.id,
                    required.display()
                );
            }
        }

        clean_generated(&pkg_dir)?;

        let src_dir = csharp_src.join(pkg.assembly);
        stage_sources(&src_dir, &pkg_dir)?;
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

        stage_native(&native, rid, pkg, &pkg_dir)?;
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

fn clean_generated(pkg_dir: &Path) -> Result<()> {
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
                || name == "package.json.meta")
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

fn stage_native(native: &Path, rid: &str, pkg: &UnityPkg, pkg_dir: &Path) -> Result<()> {
    let (prefix, ext) = rid_affix(rid);
    let file = format!("{prefix}{}.{ext}", pkg.lib);
    let src = native.join(&file);
    if !src.is_file() {
        if pkg.gpl {
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
    // Accept either an editor root or a direct Managed/UnityEngine directory.
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
