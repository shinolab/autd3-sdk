use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Subcommand;

use crate::util::{on_path, run};

#[derive(Subcommand)]
pub enum CpuCmd {
    Build,
    Flash,
    Test,
    Lint,
    Format {
        #[arg(long)]
        fix: bool,
    },
}

pub fn run_cpu(root: &Path, cmd: &CpuCmd) -> Result<()> {
    match cmd {
        CpuCmd::Build => cpu_build(root).map(|_| ()),
        CpuCmd::Flash => cpu_flash(root),
        CpuCmd::Test => cpu_test(root),
        CpuCmd::Lint => cpu_lint(root),
        CpuCmd::Format { fix } => cpu_format(root, *fix),
    }
}

const CPU_TARGET_FLAGS: &[&str] = &[
    "-mcpu=cortex-r4f",
    "-march=armv7-r",
    "-marm",
    "-mlittle-endian",
    "-mthumb-interwork",
    "-mfloat-abi=soft",
    "-mfpu=vfpv3",
];

const CPU_C_FLAGS: &[&str] = &[
    "-std=gnu11",
    "-O0",
    "-fmessage-length=0",
    "-fsigned-char",
    "-fno-exceptions",
    "-fno-unwind-tables",
    "-fno-asynchronous-unwind-tables",
];

fn cpu_flash(root: &Path) -> Result<()> {
    let bin = cpu_build(root)?;

    let jlink = match std::env::var("JLINK") {
        Ok(v) if !v.is_empty() => v,
        _ => ["JLinkExe", "JLink"]
            .into_iter()
            .find(|c| on_path(c))
            .map(str::to_string)
            .context(
                "J-Link Commander (JLinkExe) not found on PATH (install J-Link or set JLINK)",
            )?,
    };

    let script = format!(
        "r\nloadfile {} 0x30000000\nr\ng\nq\n",
        bin.to_string_lossy().replace('\\', "/")
    );
    let script_path = root.join("firmware/cpu/build/flash.jlink");
    std::fs::write(&script_path, script)
        .with_context(|| format!("writing {}", script_path.display()))?;

    run(
        &jlink,
        [
            "-device",
            "R7S910018_R4F",
            "-if",
            "JTAG",
            "-speed",
            "4000",
            "-jtagconf",
            "-1,-1",
            "-autoconnect",
            "1",
            "-ExitOnError",
            "1",
            "-CommanderScript",
            &script_path.to_string_lossy(),
        ],
        root,
    )
    .context("J-Link failed. Make sure the AUTD3 is connected and powered on.")?;
    println!("flash complete: {}", bin.display());
    Ok(())
}

pub fn cpu_build(root: &Path) -> Result<PathBuf> {
    gen_param(root)?;

    let prefix = std::env::var("CROSS_COMPILE").unwrap_or_else(|_| "arm-none-eabi-".to_string());
    let cc = format!("{prefix}gcc");
    let objcopy = format!("{prefix}objcopy");
    if std::process::Command::new(&cc)
        .arg("--version")
        .output()
        .is_err()
    {
        bail!("{cc} not found on PATH (install the Arm GNU toolchain or set CROSS_COMPILE)");
    }

    let cpu_dir = root.join("firmware/cpu");
    let platform_obj = cpu_dir.join("platform/autd3-platform.o");
    if !platform_obj.exists() {
        bail!(
            "{} not found (it is committed to the repository; check your checkout)",
            platform_obj.display()
        );
    }
    let freertos_dir = cpu_dir.join("FreeRTOS-Kernel");
    let freertos_port_dir = freertos_dir.join("portable/GCC/ARM_CRx_No_GIC");
    if !freertos_dir.join("tasks.c").exists() {
        bail!("FreeRTOS-Kernel submodule not checked out (run: git submodule update --init)");
    }
    let linker_script = cpu_dir.join("platform/autd3-cpu.ld");
    let build_dir = cpu_dir.join("build");
    let obj_dir = build_dir.join("obj");
    std::fs::create_dir_all(&obj_dir).with_context(|| format!("creating {}", obj_dir.display()))?;

    let sources = collect_build_sources(&cpu_dir, &freertos_dir, &freertos_port_dir)?;

    let inc_flags = [
        format!("-I{}", cpu_dir.join("inc").display()),
        format!("-I{}", cpu_dir.join("src").display()),
        format!("-I{}", cpu_dir.join("bsp").display()),
        format!("-I{}", freertos_dir.join("include").display()),
        format!("-I{}", freertos_port_dir.display()),
    ];

    let mut objects = Vec::new();
    for src in &sources {
        objects.push(compile_source(&cc, root, src, &obj_dir, &inc_flags)?);
    }

    let elf = build_dir.join("autd3-cpu.x");
    let map_flag = format!("-Wl,-Map={}", build_dir.join("autd3-cpu.map").display());
    let script_flag = format!("-T{}", linker_script.display());
    let platform_str = platform_obj.to_string_lossy().into_owned();
    let elf_str = elf.to_string_lossy().into_owned();
    let object_strs: Vec<String> = objects
        .iter()
        .map(|o| o.to_string_lossy().into_owned())
        .collect();
    let mut args: Vec<&str> = Vec::new();
    args.extend(CPU_TARGET_FLAGS);
    args.extend([
        "-nostartfiles",
        "--specs=nosys.specs",
        &script_flag,
        &map_flag,
        "-Wl,--no-warn-rwx-segments",
        &platform_str,
    ]);
    args.extend(object_strs.iter().map(String::as_str));
    args.extend(["-o", &elf_str]);
    run(&cc, args, root)?;

    let bin = build_dir.join("autd3-cpu.bin");
    let bin_str = bin.to_string_lossy().into_owned();
    run(
        &objcopy,
        ["-O", "binary", "--gap-fill", "0xff", &elf_str, &bin_str],
        root,
    )?;

    println!("firmware built: {}", bin.display());
    Ok(bin)
}

fn collect_build_sources(
    cpu_dir: &Path,
    freertos_dir: &Path,
    freertos_port_dir: &Path,
) -> Result<Vec<PathBuf>> {
    let mut bsp_sources = Vec::new();
    collect_c_files(&cpu_dir.join("bsp"), &mut bsp_sources)?;
    bsp_sources.retain(|p| p.extension().and_then(|e| e.to_str()) == Some("c"));
    bsp_sources.sort();

    let freertos_sources = [
        freertos_dir.join("list.c"),
        freertos_dir.join("queue.c"),
        freertos_dir.join("tasks.c"),
        freertos_dir.join("portable/MemMang/heap_1.c"),
        freertos_port_dir.join("port.c"),
        freertos_port_dir.join("portASM.S"),
    ];

    let mut app_sources = Vec::new();
    collect_c_files(&cpu_dir.join("src"), &mut app_sources)?;
    app_sources.retain(|p| p.extension().and_then(|e| e.to_str()) == Some("c"));
    app_sources.sort();
    if app_sources.is_empty() {
        bail!("no C sources found under firmware/cpu/src");
    }

    let mut sources = Vec::new();
    sources.extend(bsp_sources);
    sources.extend(freertos_sources);
    sources.extend(app_sources);
    Ok(sources)
}

fn compile_source(
    cc: &str,
    root: &Path,
    src: &Path,
    obj_dir: &Path,
    inc_flags: &[String],
) -> Result<PathBuf> {
    let rel = src.strip_prefix(root).unwrap_or(src);
    let obj = obj_dir.join(rel).with_extension("o");
    if let Some(parent) = obj.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    let src_str = src.to_string_lossy().into_owned();
    let obj_str = obj.to_string_lossy().into_owned();
    let is_asm = src.extension().and_then(|e| e.to_str()) == Some("S");
    let mut args: Vec<&str> = Vec::new();
    args.extend(CPU_TARGET_FLAGS);
    if is_asm {
        args.extend(["-x", "assembler-with-cpp"]);
    } else {
        args.extend(CPU_C_FLAGS);
    }
    args.extend(inc_flags.iter().map(String::as_str));
    args.extend(["-c", &src_str, "-o", &obj_str]);
    run(cc, args, root)?;
    Ok(obj)
}

pub(crate) fn gen_param(root: &Path) -> Result<()> {
    run("python3", ["gen_param.py"], &root.join("firmware/cpu"))
}

fn cpu_test(root: &Path) -> Result<()> {
    gen_param(root)?;

    let tests_dir = root.join("firmware/cpu/tests");
    let build_dir = tests_dir.join("build");
    let build_arg = format!("-B{}", build_dir.display());
    let source_arg = format!("-S{}", tests_dir.display());
    let build_dir_str = build_dir.to_string_lossy().into_owned();

    run("cmake", [source_arg.as_str(), build_arg.as_str()], root)?;
    run(
        "cmake",
        ["--build", build_dir_str.as_str(), "--parallel"],
        root,
    )?;
    run(
        "ctest",
        ["--test-dir", build_dir_str.as_str(), "--output-on-failure"],
        root,
    )
}

fn cpu_lint(root: &Path) -> Result<()> {
    let files = collect_cpu_sources(root)?;
    if files.is_empty() {
        bail!("no C sources found under firmware/cpu/{{src,inc}}");
    }

    let inc = root.join("firmware/cpu/inc");
    let src = root.join("firmware/cpu/src");
    let inc_flag = format!("-I{}", inc.display());
    let src_flag = format!("-I{}", src.display());

    let mut args: Vec<String> = Vec::new();
    args.push("--warnings-as-errors=*".to_string());
    args.push("--quiet".to_string());
    for f in &files {
        args.push(f.to_string_lossy().into_owned());
    }
    args.push("--".to_string());
    args.push("-std=c11".to_string());
    args.push(inc_flag);
    args.push(src_flag);

    run("clang-tidy", args.iter().map(String::as_str), root)
}

fn cpu_format(root: &Path, fix: bool) -> Result<()> {
    let files = collect_cpu_sources(root)?;
    if files.is_empty() {
        bail!("no C sources found under firmware/cpu/{{src,inc}}");
    }

    let mut args: Vec<String> = Vec::new();
    args.push("--style=file".to_string());
    if fix {
        args.push("-i".to_string());
    } else {
        args.push("--dry-run".to_string());
        args.push("-Werror".to_string());
    }
    for f in &files {
        args.push(f.to_string_lossy().into_owned());
    }

    run("clang-format", args.iter().map(String::as_str), root)
}

fn collect_cpu_sources(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for dir in ["firmware/cpu/src", "firmware/cpu/inc"] {
        collect_c_files(&root.join(dir), &mut files)?;
    }
    files.sort();
    Ok(files)
}

fn collect_c_files(path: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    let entries = std::fs::read_dir(path).with_context(|| format!("reading {}", path.display()))?;
    for entry in entries {
        let p = entry?.path();
        if p.is_dir() {
            collect_c_files(&p, files)?;
            continue;
        }
        let Some(ext) = p.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        if matches!(ext, "c" | "h") {
            files.push(p);
        }
    }
    Ok(())
}
