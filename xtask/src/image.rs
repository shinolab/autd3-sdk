use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Subcommand;

use crate::util::{capture, capture_lenient, copy_file, on_path, run};

const ECAT_INTERFACE: &str = "ecat0";
const UPLINK_INTERFACE: &str = "up0";

const SERVER_DIST: &str = "appliance/server/dist";

const DRIVER: &str = include_str!("image/driver.sh");

const GROW_MIB: u64 = 1024;

const SLACK_MIB: u64 = 128;

pub struct Board {
    dir: &'static str,
    id: &'static str,
    kernel: &'static str,
}

const BOARDS: &[Board] = &[Board {
    dir: "rp4",
    id: "raspberrypi4",
    kernel: "rpi-v8",
}];

const DEFAULT_BOARD: &str = "rp4";

const STAGED_DIST: &[&str] = &[
    "autd3-admin",
    "autd3-remote-server.service",
    "remote-server.toml",
    "autd3-wifi-init",
    "autd3-wifi-init.service",
    "run-server",
    "sudoers-autd3-admin",
    "tune-appliance.sh",
];

#[derive(Subcommand)]
pub enum ImageCmd {
    /// Check the stages without building an image (every board unless one is named)
    Lint(LintArgs),
    /// Build the appliance SD image from the published Raspberry Pi OS image (root required)
    Build(BuildArgs),
    /// Write a built image to an SD card
    Flash(FlashArgs),
}

#[derive(clap::Args)]
pub struct LintArgs {
    /// Board to check (defaults to every board under `appliance/image/`)
    #[arg(long)]
    board: Option<String>,
}

#[derive(clap::Args)]
pub struct BuildArgs {
    /// Board to build for
    #[arg(long, default_value = DEFAULT_BOARD)]
    board: String,
    /// SSH public key to authorize for the support account
    #[arg(long)]
    ssh_key: Option<PathBuf>,
    /// Password for the support account (the account is locked when this is absent)
    #[arg(long)]
    user_pass: Option<String>,
    /// Keep the uncompressed image under `.work/` so a failed build can be inspected
    #[arg(long)]
    keep_work: bool,
}

#[derive(clap::Args)]
pub struct FlashArgs {
    /// Board whose image to write
    #[arg(long, default_value = DEFAULT_BOARD)]
    board: String,
    /// Block device of the SD card, e.g. /dev/sdb or /dev/mmcblk0
    #[arg(long)]
    device: PathBuf,
    /// Image to write (defaults to the newest one under the board's deploy/)
    #[arg(long)]
    image: Option<PathBuf>,
    /// Write without asking first
    #[arg(long)]
    yes: bool,
}

pub fn run_image(root: &Path, cmd: &ImageCmd) -> Result<()> {
    match cmd {
        ImageCmd::Lint(args) => match &args.board {
            Some(name) => lint(root, board(name)?),
            None => BOARDS.iter().try_for_each(|board| lint(root, board)),
        },
        ImageCmd::Build(args) => build(root, board(&args.board)?, args),
        ImageCmd::Flash(args) => flash(root, board(&args.board)?, args),
    }
}

fn board(name: &str) -> Result<&'static Board> {
    BOARDS
        .iter()
        .find(|board| board.dir == name)
        .with_context(|| {
            format!(
                "unknown board `{name}`; appliance/image/ holds {}",
                BOARDS
                    .iter()
                    .map(|board| board.dir)
                    .collect::<Vec<_>>()
                    .join(", "),
            )
        })
}

fn image_dir(root: &Path, board: &Board) -> PathBuf {
    root.join("appliance/image").join(board.dir)
}

fn stage_dir(root: &Path, board: &Board) -> PathBuf {
    image_dir(root, board).join("stage-autd3")
}

fn read_keys(path: &Path) -> Result<BTreeMap<String, String>> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| {
            (
                key.trim().to_owned(),
                value.trim().trim_matches('"').to_owned(),
            )
        })
        .collect())
}

struct BaseImage {
    url: String,
    sha256: String,
    release: String,
}

impl BaseImage {
    fn file_name(&self) -> &str {
        self.url.rsplit('/').next().unwrap_or_default()
    }
}

fn base_image(root: &Path, board: &Board) -> Result<BaseImage> {
    let path = image_dir(root, board).join("base-image.ref");
    let keys = read_keys(&path)?;
    let get = |name: &str| {
        keys.get(name)
            .cloned()
            .with_context(|| format!("{} declares no {name}", path.display()))
    };
    let base = BaseImage {
        url: get("URL")?,
        sha256: get("SHA256")?,
        release: get("RELEASE")?,
    };
    if !base.file_name().ends_with(".img.xz") {
        bail!(
            "{} points at {}, which is not an .img.xz",
            path.display(),
            base.url,
        );
    }
    Ok(base)
}

fn shell_scripts(dir: &Path, found: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            shell_scripts(&path, found)?;
        } else if std::fs::read_to_string(&path).is_ok_and(|text| text.starts_with("#!")) {
            found.push(path);
        }
    }
    Ok(())
}

#[cfg(unix)]
fn check_syntax(scripts: &[PathBuf], root: &Path) -> Result<()> {
    for script in scripts {
        let text = std::fs::read_to_string(script)?;
        let shell = if text.starts_with("#!/bin/bash") {
            "bash"
        } else {
            "sh"
        };
        run(shell, ["-n", &script.to_string_lossy()], root)
            .with_context(|| format!("{} is not valid {shell}", script.display()))?;
    }
    println!("{} shell scripts parse", scripts.len());
    Ok(())
}

#[cfg(not(unix))]
fn check_syntax(_scripts: &[PathBuf], _root: &Path) -> Result<()> {
    println!("skipping the shell syntax check (no POSIX shell on this host)");
    Ok(())
}

fn check_referenced_files(root: &Path, board: &Board) -> Result<()> {
    let staged: Vec<&str> = STAGED_DIST
        .iter()
        .copied()
        .chain(["image-release", "autd3-remote-server", "cmdline-append.txt"])
        .collect();

    let files = stage_dir(root, board).join("files");
    let text = std::fs::read_to_string(run_script(root, board))?;
    let mut missing = Vec::new();
    for reference in text
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .flat_map(str::split_whitespace)
        .filter_map(|word| word.strip_prefix("files/"))
        .map(|word| {
            word.trim_end_matches(|c: char| {
                !(c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
            })
        })
        .filter(|word| !word.is_empty())
    {
        if files.join(reference).exists() || staged.contains(&reference) {
            continue;
        }
        missing.push(format!("files/{reference}"));
    }
    if !missing.is_empty() {
        bail!(
            "the stage installs files nothing provides: {}. Add them to the tree or stage them in \
             xtask/src/image.rs",
            missing.join(", "),
        );
    }
    println!("every file the stage installs is either tracked or staged");
    Ok(())
}

fn check_interface_names(root: &Path, board: &Board) -> Result<()> {
    let stage = stage_dir(root, board);
    let rules = std::fs::read_to_string(stage.join("files/76-autd3-interfaces.rules"))?;
    for (name, role) in [(ECAT_INTERFACE, "EtherCAT"), (UPLINK_INTERFACE, "uplink")] {
        if !rules.contains(&format!("NAME=\"{name}\"")) {
            bail!("the udev rules do not name the {role} port {name}");
        }
    }

    let expected = [
        (
            "files/10-autd3-ecat.conf",
            format!("interface-name:{ECAT_INTERFACE}"),
        ),
        (
            "files/autd3-uplink.nmconnection",
            format!("interface-name={UPLINK_INTERFACE}"),
        ),
        (
            "files/10-autd3-image.conf",
            format!("AUTD3_ECAT_IFACE={ECAT_INTERFACE}"),
        ),
    ];
    for (file, needle) in expected {
        let text = std::fs::read_to_string(stage.join(file))?;
        if !text.contains(&needle) {
            bail!("{file} does not carry `{needle}`; the port names have drifted apart");
        }
    }

    rename_bus_interface(&std::fs::read_to_string(
        root.join(SERVER_DIST).join("remote-server.toml"),
    )?)?;

    println!("the port names agree across udev, NetworkManager, the unit drop-in and the config");
    Ok(())
}

fn check_wifi_is_reachable(root: &Path, board: &Board) -> Result<()> {
    let stage = stage_dir(root, board);
    let nm_state = std::fs::read_to_string(stage.join("files/NetworkManager.state"))?;
    if !nm_state
        .lines()
        .any(|line| line.trim() == "WirelessEnabled=true")
    {
        bail!(
            "files/NetworkManager.state does not enable the radio. The published image \
             ships `WirelessEnabled=false` unless a regulatory domain was chosen, and the \
             read-only rootfs means nothing done at runtime survives a reboot",
        );
    }

    let run = std::fs::read_to_string(run_script(root, board))?;
    for needle in [
        "/var/lib/NetworkManager/NetworkManager.state",
        "systemctl enable autd3-wifi-init.service",
    ] {
        if !run.contains(needle) {
            bail!("run.sh no longer carries `{needle}`; Wi-Fi would stay down");
        }
    }

    println!("the image ships the Wi-Fi radio enabled and applies the stored regulatory domain");
    Ok(())
}

fn check_keyfile_enums(root: &Path, board: &Board) -> Result<()> {
    const NUMERIC: &[&str] = &["link-local", "dhcp-timeout", "route-metric", "dad-timeout"];
    let mut checked = 0;
    for entry in std::fs::read_dir(stage_dir(root, board).join("files"))? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("nmconnection") {
            continue;
        }
        checked += 1;
        let text = std::fs::read_to_string(&path)?;
        for line in text.lines().map(str::trim) {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            if NUMERIC.contains(&key) && value.trim().parse::<i64>().is_err() {
                bail!(
                    "{}: {key} must be a number in a keyfile, got `{}`. NetworkManager drops the \
                     setting and says so in one log line nobody reads",
                    path.display(),
                    value.trim(),
                );
            }
        }
    }
    println!("{checked} NetworkManager keyfiles use numbers where the format wants them");
    Ok(())
}

fn check_kernel_flavor(root: &Path, board: &Board) -> Result<()> {
    let script = std::fs::read_to_string(run_script(root, board))?;
    if !script.contains(&format!("-{}$", board.kernel)) {
        bail!(
            "run.sh does not pick the -{} kernel. The build strips every other flavour out of the \
             published image, so the stage would find no matching modules",
            board.kernel,
        );
    }
    println!(
        "the stage builds its initramfs for the -{} kernel the image ships",
        board.kernel,
    );
    Ok(())
}

fn run_script(root: &Path, board: &Board) -> PathBuf {
    stage_dir(root, board).join("run.sh")
}

fn check_chroot_capabilities(root: &Path, board: &Board) -> Result<()> {
    let script = run_script(root, board);
    let text = std::fs::read_to_string(&script)?;
    let mut delimiter: Option<String> = None;
    for line in text.lines() {
        match &delimiter {
            Some(end) if line.trim() == end => delimiter = None,
            Some(_) if line.trim_start().starts_with("setcap ") => {
                bail!(
                    "{} calls setcap inside an on_chroot block. The driver drops CAP_SETFCAP \
                     there; run it against \"${{ROOTFS_DIR}}/...\" from outside instead",
                    script.display(),
                );
            }
            Some(_) => {}
            None => {
                if let Some((_, tail)) = line.split_once("on_chroot <<") {
                    delimiter = Some(tail.trim().trim_matches('\'').trim_matches('"').to_owned());
                }
            }
        }
    }
    println!("the stage asks the chroot for no capability it cannot have");
    Ok(())
}

fn check_stage_layout(root: &Path, board: &Board) -> Result<()> {
    let packages = stage_dir(root, board).join("packages");
    if !packages.is_file() {
        bail!(
            "{} is missing; the driver reads the stage's package list from it",
            packages.display(),
        );
    }

    let script = run_script(root, board);
    if !script.is_file() {
        bail!(
            "{} is missing; the driver runs it as the stage",
            script.display()
        );
    }
    if !is_executable(&script) {
        bail!(
            "{} is not executable; the build would stop at it",
            script.display()
        );
    }
    println!("the stage holds an executable run.sh next to its package list");
    Ok(())
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|meta| meta.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(_path: &Path) -> bool {
    true
}

fn lint(root: &Path, board: &Board) -> Result<()> {
    println!("checking the {} image", board.dir);
    let base = base_image(root, board)?;
    println!("built from {} ({})", base.file_name(), base.release);

    check_stage_layout(root, board)?;

    let mut scripts = Vec::new();
    shell_scripts(&stage_dir(root, board), &mut scripts)?;
    shell_scripts(&root.join(SERVER_DIST), &mut scripts)?;
    scripts.sort();
    check_syntax(&scripts, root)?;

    check_referenced_files(root, board)?;
    check_interface_names(root, board)?;
    check_wifi_is_reachable(root, board)?;
    check_keyfile_enums(root, board)?;
    check_kernel_flavor(root, board)?;
    check_chroot_capabilities(root, board)?;

    let template = std::fs::read_to_string(image_dir(root, board).join("config.in"))?;
    let known = ["@USER_PASS@", "@LOCK_ACCOUNT@", "@SSH_PUBKEY@"];
    for placeholder in template
        .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '@'))
        .filter(|word| word.starts_with('@') && word.ends_with('@') && word.len() > 2)
    {
        if !known.contains(&placeholder) {
            bail!("config.in uses {placeholder}, which the builder does not substitute");
        }
    }
    println!("config.in only uses placeholders the builder fills");

    check_driver_reads_config(&template)?;

    let script = run_script(root, board);
    let text = std::fs::read_to_string(&script)?;
    for name in text
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|word| word.starts_with("AUTD3_") && word.len() > 6)
    {
        if !template.contains(&format!("export {name}=")) {
            bail!(
                "{} reads {name}, which config.in does not export",
                script.display(),
            );
        }
    }
    println!("every config variable the stage reads is exported");
    Ok(())
}

const CONFIG_SETTINGS: &[&str] = &[
    "TARGET_HOSTNAME",
    "LOCALE_DEFAULT",
    "KEYBOARD_KEYMAP",
    "TIMEZONE_DEFAULT",
    "FIRST_USER_NAME",
    "FIRST_USER_PASS",
    "AUTD3_LOCK_ACCOUNT",
];

fn check_driver_reads_config(template: &str) -> Result<()> {
    for name in CONFIG_SETTINGS {
        let declared = template.lines().any(|line| {
            line.trim_start()
                .trim_start_matches("export ")
                .starts_with(&format!("{name}="))
        });
        if !declared {
            bail!("config.in declares no {name}, and the driver reads it unset");
        }
        if !DRIVER.contains(&format!("${{{name}")) {
            bail!(
                "driver.sh no longer reads {name}; take it out of config.in and out of \
                 CONFIG_SETTINGS rather than leaving a setting that does nothing",
            );
        }
        if !DRIVER
            .lines()
            .filter_map(|line| line.trim().strip_prefix("export "))
            .any(|line| line.split(['=', ' ']).any(|word| word == *name))
        {
            bail!(
                "driver.sh sources {name} but does not export it, so a stage script that reads it \
                 sees the empty string",
            );
        }
    }
    println!(
        "{} settings reach the driver from config.in",
        CONFIG_SETTINGS.len(),
    );
    Ok(())
}

fn rename_bus_interface(config: &str) -> Result<String> {
    let mut renamed = String::with_capacity(config.len());
    let mut found = false;
    for line in config.lines() {
        if line.trim_start().starts_with("interface") && line.contains('=') {
            renamed.push_str("interface = \"");
            renamed.push_str(ECAT_INTERFACE);
            renamed.push('"');
            found = true;
        } else {
            renamed.push_str(line);
        }
        renamed.push('\n');
    }
    if !found {
        bail!(
            "{SERVER_DIST}/remote-server.toml declares no `interface`, so the image cannot point \
             it at {ECAT_INTERFACE}",
        );
    }
    Ok(renamed)
}

fn appliance_version(root: &Path) -> Result<String> {
    let component = crate::component::COMPONENTS
        .iter()
        .find(|c| c.name == "software")
        .context("no `software` component")?;
    component.current_version(root)
}

fn commit(root: &Path) -> String {
    capture_lenient("git", &["rev-parse", "--short", "HEAD"], root)
        .ok()
        .filter(|sha| !sha.is_empty())
        .unwrap_or_else(|| "unknown".to_owned())
}

fn built_on(root: &Path) -> String {
    capture_lenient("git", &["log", "-1", "--format=%cs"], root)
        .ok()
        .filter(|date| !date.is_empty())
        .unwrap_or_else(|| "unknown".to_owned())
}

fn stage_files(root: &Path, board: &Board, version: &str) -> Result<()> {
    let files = stage_dir(root, board).join("files");
    let dist = root.join(SERVER_DIST);

    let binary = crate::server::cross_build(root)?;
    copy_file(&binary, &files.join("autd3-remote-server"))?;

    for name in STAGED_DIST {
        copy_file(&dist.join(name), &files.join(name))?;
    }

    let seed = files.join("remote-server.toml");
    let text = std::fs::read_to_string(&seed)?;
    let renamed = rename_bus_interface(&text)?;
    std::fs::write(&seed, renamed)?;

    let example = std::fs::read_to_string(dist.join("cmdline.txt.example"))?;
    let params: Vec<&str> = example
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    if params.len() != 1 {
        bail!(
            "{SERVER_DIST}/cmdline.txt.example should hold exactly one uncommented line, found {}",
            params.len(),
        );
    }
    std::fs::write(files.join("cmdline-append.txt"), params[0].as_bytes())?;

    let stamp = format!(
        "# Written by `cargo xtask image build`. Read by GET /status.\n\
         IMAGE_VERSION=\"{version}\"\n\
         SDK_VERSION=\"{sdk}\"\n\
         BUILT=\"{built}\"\n\
         BOARD=\"{board}\"\n\
         COMMIT=\"{commit}\"\n",
        board = board.id,
        sdk = appliance_version(root)?,
        built = built_on(root),
        commit = commit(root),
    );
    std::fs::write(files.join("image-release"), stamp)?;
    Ok(())
}

fn tool_available(tool: &str) -> bool {
    on_path(tool)
        || ["/usr/local/sbin", "/usr/sbin", "/sbin"]
            .iter()
            .any(|dir| Path::new(dir).join(tool).is_file())
}

fn cache_dir(root: &Path, board: &Board) -> PathBuf {
    image_dir(root, board).join(".cache")
}

fn work_dir(root: &Path, board: &Board) -> PathBuf {
    image_dir(root, board).join(".work")
}

fn sha256(path: &Path, root: &Path) -> Result<String> {
    let output = capture("sha256sum", &[&path.to_string_lossy()], root)?;
    Ok(output
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_owned())
}

fn fetch_base_image(root: &Path, board: &Board, base: &BaseImage) -> Result<PathBuf> {
    let dir = cache_dir(root, board);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(base.file_name());

    if path.is_file() && sha256(&path, root)? == base.sha256 {
        println!("using the cached {}", base.file_name());
        return Ok(path);
    }
    if path.exists() {
        println!(
            "the cached {} does not match its pin; fetching again",
            base.file_name()
        );
        std::fs::remove_file(&path)?;
    }

    println!("fetching {}", base.url);
    let partial = path.with_extension("part");
    run(
        "curl",
        [
            "--fail",
            "--location",
            "--retry",
            "3",
            "--progress-bar",
            "--output",
            &partial.to_string_lossy(),
            &base.url,
        ],
        root,
    )?;
    let got = sha256(&partial, root)?;
    if got != base.sha256 {
        bail!(
            "{} hashes to {got}, not the pinned {}. Read what changed before moving SHA256 in \
             base-image.ref",
            base.file_name(),
            base.sha256,
        );
    }
    std::fs::rename(&partial, &path)?;
    Ok(path)
}

fn unpack_base_image(root: &Path, board: &Board, cached: &Path) -> Result<PathBuf> {
    let dir = work_dir(root, board);
    std::fs::create_dir_all(&dir)?;
    let image = dir.join("autd3.img");
    println!(
        "unpacking {}",
        cached.file_name().unwrap_or_default().to_string_lossy()
    );
    let script = format!(
        "xz -dc -T0 {from} > {to}",
        from = shell_quote(cached),
        to = shell_quote(&image),
    );
    run("sh", ["-c", &script], root)?;
    let grown = std::fs::metadata(&image)?.len() + GROW_MIB * 1024 * 1024;
    run(
        "truncate",
        ["-s", &grown.to_string(), &image.to_string_lossy()],
        root,
    )?;
    Ok(image)
}

fn compress_image(root: &Path, image: &Path, deploy: &Path, name: &str) -> Result<PathBuf> {
    std::fs::create_dir_all(deploy)?;
    let out = deploy.join(format!("{name}.img.xz"));
    println!("compressing to {}", out.display());
    let script = format!(
        "xz -c -T0 -6 {from} > {to}",
        from = shell_quote(image),
        to = shell_quote(&out),
    );
    run("sh", ["-c", &script], root)?;
    Ok(out)
}

fn run_driver(root: &Path, board: &Board, image: &Path, config: &Path) -> Result<()> {
    let work = work_dir(root, board);
    let driver = work.join("driver.sh");
    std::fs::write(&driver, DRIVER)?;

    if capture_lenient("id", &["-u"], root)
        .unwrap_or_default()
        .trim()
        == "0"
    {
        return run(
            "bash",
            [
                driver.as_os_str(),
                config.as_os_str(),
                image.as_os_str(),
                stage_dir(root, board).as_os_str(),
                work.as_os_str(),
                std::ffi::OsStr::new(board.kernel),
                std::ffi::OsStr::new(&SLACK_MIB.to_string()),
            ],
            root,
        );
    }
    println!("the loop device, the mounts and the chroot need root; asking sudo once");
    run(
        "sudo",
        [
            std::ffi::OsStr::new("bash"),
            driver.as_os_str(),
            config.as_os_str(),
            image.as_os_str(),
            stage_dir(root, board).as_os_str(),
            work.as_os_str(),
            std::ffi::OsStr::new(board.kernel),
            std::ffi::OsStr::new(&SLACK_MIB.to_string()),
        ],
        root,
    )
}

fn ssh_pubkey(path: &Path) -> Result<String> {
    let key = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?
        .trim()
        .to_owned();
    if key.contains('"') || key.contains('\n') {
        bail!(
            "{} does not look like a single SSH public key",
            path.display()
        );
    }
    Ok(key)
}

fn checked_user_pass(pass: &str) -> Result<String> {
    if pass.is_empty() {
        bail!("--user-pass must not be empty; leave it out to lock the account instead");
    }
    if pass
        .chars()
        .any(|c| c.is_control() || matches!(c, '"' | '\\' | '`' | '$'))
    {
        bail!(
            "--user-pass is written into a config file bash sources, so it must not contain \
             a double quote, a backslash, a backtick, `$` or a control character"
        );
    }
    Ok(pass.to_owned())
}

fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', r"'\''"))
}

const BINFMT_MISC: &str = "/proc/sys/fs/binfmt_misc";

const EMULATION_FIX: &str = "Register a statically linked emulator:\n  \
     Debian/Ubuntu: sudo apt install qemu-user-static\n  \
     Arch:          sudo pacman -S qemu-user-static-binfmt  (replaces qemu-user-binfmt)\n  \
     Fedora:        sudo dnf install qemu-user-static\n  \
     any distro:    docker run --privileged --rm tonistiigi/binfmt --install arm64\n\
     Then `sudo systemctl restart systemd-binfmt` (or reboot) and re-run. Any leftover \
     registration naming a dynamically linked interpreter has to go: when two handlers claim \
     the same magic, which one wins is registration order.";

fn is_statically_linked(path: &Path) -> Result<bool> {
    let elf = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let word = |at: usize, len: usize| -> u64 {
        elf.get(at..at + len).map_or(0, |bytes| {
            bytes
                .iter()
                .rev()
                .fold(0u64, |value, byte| (value << 8) | u64::from(*byte))
        })
    };
    if elf.get(..4) != Some(b"\x7fELF") || elf.get(4) != Some(&2) || elf.get(5) != Some(&1) {
        return Ok(true);
    }
    let (offset, entry_size, count) = (
        usize::try_from(word(0x20, 8))?,
        usize::from(u16::try_from(word(0x36, 2))?),
        usize::from(u16::try_from(word(0x38, 2))?),
    );
    Ok(!(0..count).any(|i| word(offset + i * entry_size, 4) == 3))
}

fn check_emulation(root: &Path) -> Result<()> {
    let arch = capture_lenient("uname", &["-m"], root).unwrap_or_default();
    if arch.starts_with("aarch64") || arch.starts_with("arm") {
        return Ok(());
    }
    if !Path::new(BINFMT_MISC).join("register").exists() {
        bail!(
            "binfmt_misc is not mounted. Run `sudo modprobe binfmt_misc` and, if that is not \
             enough, `sudo mount binfmt_misc -t binfmt_misc {BINFMT_MISC}`",
        );
    }

    let mut handlers = Vec::new();
    for entry in std::fs::read_dir(BINFMT_MISC).with_context(|| format!("reading {BINFMT_MISC}"))? {
        let path = entry?.path();
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if !(name.contains("aarch64") || name.contains("arm64")) || name.ends_with("_be") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let field = |key: &str| {
            text.lines()
                .find_map(|line| line.strip_prefix(key))
                .unwrap_or_default()
                .trim()
                .to_owned()
        };
        handlers.push((name.into_owned(), field("interpreter "), field("flags:")));
    }
    if handlers.is_empty() {
        bail!("no binfmt_misc handler for aarch64 is registered.\n{EMULATION_FIX}");
    }

    for (name, interpreter, flags) in &handlers {
        if !flags.contains('F') {
            bail!(
                "the binfmt_misc handler `{name}` lacks the F flag, so the kernel looks the \
                 interpreter up under the chroot, where an x86 emulator does not \
                 exist.\n{EMULATION_FIX}",
            );
        }
        let interpreter = Path::new(interpreter);
        if !is_statically_linked(interpreter)? {
            bail!(
                "the binfmt_misc handler `{name}` runs {}, which is dynamically linked. Its \
                 loader and its shared libraries are resolved under the chroot, and an arm64 \
                 rootfs holds neither.\n{EMULATION_FIX}",
                interpreter.display(),
            );
        }
    }
    Ok(())
}

fn build(root: &Path, board: &Board, args: &BuildArgs) -> Result<()> {
    if !cfg!(target_os = "linux") {
        bail!("the build mounts an ext4 filesystem and chroots into it; that only works on Linux");
    }
    for tool in [
        "curl",
        "xz",
        "sha256sum",
        "sfdisk",
        "losetup",
        "e2fsck",
        "resize2fs",
        "dumpe2fs",
        "capsh",
        "setcap",
    ] {
        if !tool_available(tool) {
            bail!("`{tool}` is not installed (util-linux, e2fsprogs, xz-utils, libcap)");
        }
    }
    check_emulation(root)?;
    lint(root, board)?;

    let base = base_image(root, board)?;
    let version = format!(
        "{}-{}",
        appliance_version(root)?,
        built_on(root).replace('-', "")
    );
    let name = format!("autd3-appliance-{}-{version}", board.dir);
    println!(
        "building {name} from {} ({})",
        base.file_name(),
        base.release,
    );

    stage_files(root, board, &version)?;
    let cached = fetch_base_image(root, board, &base)?;

    let (user_pass, lock) = match &args.user_pass {
        None => ("locked-by-the-autd3-stage".to_owned(), "1"),
        Some(pass) => (checked_user_pass(pass)?, "0"),
    };
    let ssh = match &args.ssh_key {
        Some(path) => format!("PUBKEY_SSH_FIRST_USER=\"{}\"", ssh_pubkey(path)?),
        None => "# no SSH key was given to the build".to_owned(),
    };
    if args.ssh_key.is_none() && args.user_pass.is_none() {
        println!(
            "note: the image will have no shell access at all. The control API and reflashing the \
             card are the only ways in; pass --ssh-key to keep a way back."
        );
    }

    let image = unpack_base_image(root, board, &cached)?;
    let config = work_dir(root, board).join("config.sh");
    std::fs::write(
        &config,
        std::fs::read_to_string(image_dir(root, board).join("config.in"))?
            .replace("@USER_PASS@", &user_pass)
            .replace("@LOCK_ACCOUNT@", lock)
            .replace("@SSH_PUBKEY@", &ssh),
    )?;

    run_driver(root, board, &image, &config)?;

    let deploy = image_dir(root, board).join("deploy");
    discard_stale_images(std::slice::from_ref(&deploy))?;
    let out = compress_image(root, &image, &deploy, &name)?;
    if args.keep_work {
        println!("the uncompressed image is still at {}", image.display());
    } else {
        std::fs::remove_file(&image).ok();
    }
    std::fs::remove_file(config).ok();

    println!("built {}", out.display());
    Ok(())
}

fn discard_stale_images(dirs: &[PathBuf]) -> Result<()> {
    for dir in dirs.iter().filter(|dir| dir.is_dir()) {
        for image in newest_images(dir)? {
            std::fs::remove_file(&image)
                .with_context(|| format!("removing the previous {}", image.display()))?;
        }
    }
    Ok(())
}

fn newest_images(deploy: &Path) -> Result<Vec<PathBuf>> {
    let mut images: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(deploy)
        .with_context(|| format!("reading {}", deploy.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.to_string_lossy().ends_with(".img.xz"))
        .filter_map(|path| {
            let modified = path.metadata().ok()?.modified().ok()?;
            Some((modified, path))
        })
        .collect();
    images.sort();
    images.reverse();
    Ok(images.into_iter().map(|(_, path)| path).collect())
}

fn check_whole_disk(root: &Path, device: &Path) -> Result<()> {
    let kind = capture_lenient("lsblk", &["-ndo", "TYPE", &device.to_string_lossy()], root)
        .unwrap_or_default()
        .trim()
        .to_owned();
    match kind.as_str() {
        "disk" | "loop" => Ok(()),
        "part" => {
            let whole = capture_lenient(
                "lsblk",
                &["-ndo", "PKNAME", &device.to_string_lossy()],
                root,
            )
            .unwrap_or_default()
            .trim()
            .to_owned();
            bail!(
                "{} is a partition, not the card. The image brings its own partition table, so it \
                 has to be written to the whole device{}",
                device.display(),
                if whole.is_empty() {
                    String::new()
                } else {
                    format!(" -- /dev/{whole}")
                },
            )
        }
        "" => bail!(
            "lsblk does not recognise {} as a block device",
            device.display(),
        ),
        other => bail!(
            "{} is a {other}, and the image has to be written to a whole card",
            device.display(),
        ),
    }
}

fn flash(root: &Path, board: &Board, args: &FlashArgs) -> Result<()> {
    let deploy = image_dir(root, board).join("deploy");
    let image = match &args.image {
        Some(path) => path.clone(),
        None => newest_images(&deploy)?
            .into_iter()
            .next()
            .with_context(|| {
                format!(
                    "no .img.xz under {}; run `cargo xtask image build --board {}` first",
                    deploy.display(),
                    board.dir,
                )
            })?,
    };
    if !args.device.exists() {
        bail!("{} does not exist", args.device.display());
    }
    check_whole_disk(root, &args.device)?;

    let description = capture_lenient(
        "lsblk",
        &["-no", "NAME,SIZE,MODEL", &args.device.to_string_lossy()],
        root,
    )
    .unwrap_or_default();
    println!("about to overwrite {} {description}", args.device.display());
    println!("with {}", image.display());
    if !args.yes {
        bail!("refusing to write without --yes");
    }

    let script = format!(
        "set -e\n\
         xz -dc {image} | dd of={device} bs=4M status=progress conv=fsync\n\
         sync\n\
         blockdev --flushbufs {device} || true\n\
         sysctl -q -w vm.drop_caches=3\n\
         size=$(xz --robot -l {image} | awk '$1 == \"totals\" {{ print $5 }}')\n\
         echo \"verifying ${{size}} bytes\"\n\
         written=$(head -c \"${{size}}\" {device} | sha256sum | cut -d\" \" -f1)\n\
         wanted=$(xz -dc {image} | sha256sum | cut -d\" \" -f1)\n\
         [ \"${{written}}\" = \"${{wanted}}\" ] || {{\n\
           echo \"the card reads back as ${{written}}, not ${{wanted}}\" >&2\n\
           exit 1\n\
         }}",
        image = shell_quote(&image),
        device = shell_quote(&args.device),
    );
    run("sudo", ["sh", "-c", &script], root).context(
        "the card does not read back as the image that was written to it. The write reported \
         success, so suspect the path it took rather than the image: a card reader behind a hub \
         or dock, the cable, or the card. Writing through a port on the machine itself is the \
         quickest thing to rule out",
    )?;
    println!("written and verified; the card is ready");
    Ok(())
}
