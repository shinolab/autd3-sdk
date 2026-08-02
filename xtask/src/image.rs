use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Subcommand;

use crate::util::{capture, capture_lenient, copy_dir, copy_file, on_path, run};

const PI_GEN_URL: &str = "https://github.com/RPi-Distro/pi-gen";
const ECAT_INTERFACE: &str = "ecat0";
const UPLINK_INTERFACE: &str = "up0";

const SERVER_DIST: &str = "appliance/server/dist";

pub struct Board {
    /// Directory under `appliance/image/`
    dir: &'static str,
    /// Value the image reports as `BOARD` through `GET /status`
    id: &'static str,
    /// Kernel flavour the board boots, as Raspberry Pi OS names it
    kernel: &'static str,
}

const BOARDS: &[Board] = &[Board {
    dir: "rp4",
    id: "raspberrypi4",
    kernel: "rpi-v8",
}];

const DEFAULT_BOARD: &str = "rp4";

const STAGED_DIST: &[(&str, &str)] = &[
    ("autd3-admin", "01-appliance/files/autd3-admin"),
    (
        "autd3-remote-server.service",
        "01-appliance/files/autd3-remote-server.service",
    ),
    (
        "remote-server.toml",
        "01-appliance/files/remote-server.toml",
    ),
    ("autd3-wifi-init", "02-network/files/autd3-wifi-init"),
    (
        "autd3-wifi-init.service",
        "02-network/files/autd3-wifi-init.service",
    ),
    ("run-server", "01-appliance/files/run-server"),
    (
        "sudoers-autd3-admin",
        "01-appliance/files/sudoers-autd3-admin",
    ),
    ("tune-appliance.sh", "01-appliance/files/tune-appliance.sh"),
];

#[derive(Subcommand)]
pub enum ImageCmd {
    /// Check the pi-gen stages without building an image (every board unless one is named)
    Lint(LintArgs),
    /// Build the appliance SD image with pi-gen (Docker required)
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
    /// Keep pi-gen's container so a failed build can be inspected (it holds the work volume)
    #[arg(long)]
    keep_container: bool,
    /// Reuse the previous build's container instead of starting from debootstrap. The stages whose
    /// rootfs is already there are skipped, which is the difference between minutes and an hour.
    #[arg(long)]
    resume: bool,
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

struct PiGenPin {
    git_ref: String,
    sha: String,
    release: String,
}

fn pi_gen_pin(root: &Path, board: &Board) -> Result<PiGenPin> {
    let path = image_dir(root, board).join("pi-gen.ref");
    let keys = read_keys(&path)?;
    let get = |name: &str| {
        keys.get(name)
            .cloned()
            .with_context(|| format!("{} declares no {name}", path.display()))
    };
    Ok(PiGenPin {
        git_ref: get("REF")?,
        sha: get("SHA")?,
        release: get("RELEASE")?,
    })
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
        .map(|(_, to)| *to)
        .chain(["01-appliance/files/image-release"])
        .chain(["01-appliance/files/autd3-remote-server"])
        .chain(["03-system/files/cmdline-append.txt"])
        .collect();

    let stage = stage_dir(root, board);
    let mut missing = Vec::new();
    for substage in sorted_entries(&stage)? {
        let script = substage.join("00-run.sh");
        if !script.is_file() {
            continue;
        }
        let name = substage.file_name().unwrap_or_default().to_string_lossy();
        let text = std::fs::read_to_string(&script)?;
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
            let relative = format!("{name}/files/{reference}");
            if substage.join("files").join(reference).exists()
                || staged.contains(&relative.as_str())
            {
                continue;
            }
            missing.push(relative);
        }
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
    let rules = std::fs::read_to_string(stage.join("02-network/files/76-autd3-interfaces.rules"))?;
    for (name, role) in [(ECAT_INTERFACE, "EtherCAT"), (UPLINK_INTERFACE, "uplink")] {
        if !rules.contains(&format!("NAME=\"{name}\"")) {
            bail!("the udev rules do not name the {role} port {name}");
        }
    }

    let expected = [
        (
            "02-network/files/10-autd3-ecat.conf",
            format!("interface-name:{ECAT_INTERFACE}"),
        ),
        (
            "02-network/files/autd3-uplink.nmconnection",
            format!("interface-name={UPLINK_INTERFACE}"),
        ),
        (
            "03-system/files/10-autd3-image.conf",
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
    let state = std::fs::read_to_string(stage.join("02-network/files/NetworkManager.state"))?;
    if !state
        .lines()
        .any(|line| line.trim() == "WirelessEnabled=true")
    {
        bail!(
            "02-network/files/NetworkManager.state does not enable the radio. pi-gen's stage2 \
             writes `WirelessEnabled=false` when the build sets no WPA_COUNTRY, and the \
             read-only rootfs means nothing done at runtime survives a reboot",
        );
    }

    let run = std::fs::read_to_string(stage.join("02-network/00-run.sh"))?;
    for needle in [
        "/var/lib/NetworkManager/NetworkManager.state",
        "systemctl enable autd3-wifi-init.service",
    ] {
        if !run.contains(needle) {
            bail!("02-network/00-run.sh no longer carries `{needle}`; Wi-Fi would stay down");
        }
    }

    println!("the image ships the Wi-Fi radio enabled and applies the stored regulatory domain");
    Ok(())
}

fn check_keyfile_enums(root: &Path, board: &Board) -> Result<()> {
    const NUMERIC: &[&str] = &["link-local", "dhcp-timeout", "route-metric", "dad-timeout"];
    let stage = stage_dir(root, board);
    let mut checked = 0;
    for substage in sorted_entries(&stage)? {
        let files = substage.join("files");
        if !files.is_dir() {
            continue;
        }
        for entry in std::fs::read_dir(&files)? {
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
                        "{}: {key} must be a number in a keyfile, got `{}`. NetworkManager drops \
                         the setting and says so in one log line nobody reads",
                        path.display(),
                        value.trim(),
                    );
                }
            }
        }
    }
    println!("{checked} NetworkManager keyfiles use numbers where the format wants them");
    Ok(())
}

fn check_kernel_flavor(root: &Path, board: &Board) -> Result<()> {
    let script = std::fs::read_to_string(stage_dir(root, board).join("04-overlay/00-run.sh"))?;
    if !script.contains(&format!("-{}$", board.kernel)) {
        bail!(
            "04-overlay/00-run.sh does not pick the -{} kernel. The build strips every other \
             flavour out of pi-gen's stage0, so the stage would find no matching modules",
            board.kernel,
        );
    }
    println!(
        "the overlay stage builds its initramfs for the -{} kernel the image ships",
        board.kernel,
    );
    Ok(())
}

fn sorted_entries(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_dir())
        .collect();
    entries.sort();
    Ok(entries)
}

fn run_scripts(root: &Path, board: &Board) -> Result<Vec<PathBuf>> {
    let stage = stage_dir(root, board);
    let mut scripts = vec![stage.join("prerun.sh")];
    scripts.extend(sorted_entries(&stage)?.iter().map(|d| d.join("00-run.sh")));
    scripts.retain(|path| path.is_file());
    Ok(scripts)
}

fn check_chroot_capabilities(root: &Path, board: &Board) -> Result<()> {
    for script in run_scripts(root, board)? {
        let text = std::fs::read_to_string(&script)?;
        let mut delimiter: Option<String> = None;
        for line in text.lines() {
            match &delimiter {
                Some(end) if line.trim() == end => delimiter = None,
                Some(_) if line.trim_start().starts_with("setcap ") => {
                    bail!(
                        "{} calls setcap inside an on_chroot block. pi-gen drops CAP_SETFCAP \
                         there; run it against \"${{ROOTFS_DIR}}/...\" from outside instead",
                        script.display(),
                    );
                }
                Some(_) => {}
                None => {
                    if let Some((_, tail)) = line.split_once("on_chroot <<") {
                        delimiter =
                            Some(tail.trim().trim_matches('\'').trim_matches('"').to_owned());
                    }
                }
            }
        }
    }
    println!("no stage asks the chroot for a capability it cannot have");
    Ok(())
}

fn check_stage_layout(root: &Path, board: &Board) -> Result<()> {
    let stage = stage_dir(root, board);
    if stage.join("00-packages").is_file() {
        bail!(
            "stage-autd3/00-packages is a file. pi-gen only reads package lists inside a \
             sub-stage, so this one is silently ignored; move it to 00-packages/00-packages",
        );
    }

    for script in run_scripts(root, board)? {
        if !is_executable(&script) {
            bail!(
                "{} is not executable; pi-gen would skip it without saying so",
                script.display(),
            );
        }
    }
    println!("every stage script is executable and the package list is in a sub-stage");
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
    let pin = pi_gen_pin(root, board)?;
    println!("pi-gen {} ({})", pin.git_ref, pin.release);

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
    let known = [
        "@IMG_NAME@",
        "@RELEASE@",
        "@USER_PASS@",
        "@LOCK_ACCOUNT@",
        "@SSH_PUBKEY@",
    ];
    for placeholder in template
        .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '@'))
        .filter(|word| word.starts_with('@') && word.ends_with('@') && word.len() > 2)
    {
        if !known.contains(&placeholder) {
            bail!("config.in uses {placeholder}, which the builder does not substitute");
        }
    }
    println!("config.in only uses placeholders the builder fills");

    let stage_name = stage_dir(root, board)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let stage_list = template
        .lines()
        .find_map(|line| line.trim().strip_prefix("STAGE_LIST="))
        .unwrap_or_default()
        .trim_matches('"');
    if stage_list.split_whitespace().last() != Some(stage_name.as_str()) {
        bail!(
            "STAGE_LIST must end with the bare name `{stage_name}`, not a path: the build runs in \
             a container that has the stage copied to /pi-gen/{stage_name}",
        );
    }
    println!("STAGE_LIST names the stage the way the container will see it");

    for script in run_scripts(root, board)? {
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
    }
    println!("every config variable the stage reads is exported");
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
    let stage = stage_dir(root, board);
    let dist = root.join(SERVER_DIST);

    let binary = crate::server::cross_build(root)?;
    copy_file(
        &binary,
        &stage.join("01-appliance/files/autd3-remote-server"),
    )?;

    for (from, to) in STAGED_DIST {
        copy_file(&dist.join(from), &stage.join(to))?;
    }

    let seed = stage.join("01-appliance/files/remote-server.toml");
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
    std::fs::write(
        stage.join("03-system/files/cmdline-append.txt"),
        params[0].as_bytes(),
    )?;

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
    std::fs::write(stage.join("01-appliance/files/image-release"), stamp)?;
    Ok(())
}

fn pi_gen_checkout(root: &Path, board: &Board, pin: &PiGenPin) -> Result<PathBuf> {
    let dir = image_dir(root, board).join(".pi-gen");
    if dir.join(".git").is_dir() {
        run(
            "git",
            ["fetch", "--tags", "--depth", "1", "origin", &pin.git_ref],
            &dir,
        )?;
        run("git", ["checkout", "--force", "FETCH_HEAD"], &dir)?;
    } else {
        std::fs::create_dir_all(&dir)?;
        run(
            "git",
            [
                "clone",
                "--depth",
                "1",
                "--branch",
                &pin.git_ref,
                PI_GEN_URL,
                &dir.to_string_lossy(),
            ],
            root,
        )?;
    }
    let head = capture("git", &["rev-parse", "HEAD"], &dir)?.trim().to_owned();
    if head != pin.sha {
        bail!(
            "pi-gen {} is {head}, not the pinned {}. A tag that moved is a different image; \
             update SHA in pi-gen.ref once the new contents have been read",
            pin.git_ref,
            pin.sha,
        );
    }
    std::fs::write(dir.join("stage2/SKIP_IMAGES"), "")?;
    keep_one_kernel(&dir, board)?;
    fix_loopdev_helpers(&dir)?;

    let staged = dir.join("stage-autd3");
    if staged.exists() {
        std::fs::remove_dir_all(&staged)
            .with_context(|| format!("clearing {}", staged.display()))?;
    }
    copy_dir(&stage_dir(root, board), &staged)?;
    Ok(dir)
}

fn keep_one_kernel(pi_gen: &Path, board: &Board) -> Result<()> {
    let list = pi_gen.join("stage0/02-firmware/01-packages");
    let text = std::fs::read_to_string(&list)
        .with_context(|| format!("reading {}", list.display()))?;
    let wanted = format!("linux-image-{}", board.kernel);
    if !text.lines().any(|line| line.trim() == wanted) {
        bail!(
            "pi-gen's stage0 no longer installs {wanted}; the {} board would boot a kernel the \
             image does not ship",
            board.dir,
        );
    }
    let mut kept = String::with_capacity(text.len());
    for line in text.lines().filter(|line| {
        let name = line.trim();
        !(name.starts_with("linux-image-") || name.starts_with("linux-headers-"))
    }) {
        kept.push_str(line);
        kept.push('\n');
    }
    kept.push_str(&wanted);
    kept.push('\n');
    std::fs::write(&list, kept)?;
    println!("stage0 installs {wanted} alone (no other flavour, no kernel headers)");
    Ok(())
}

const LOOPDEV_EDITS: &[(&str, &str, &str)] = &[
    (
        r#"loopdev="$(losetup -f)""#,
        r#"loopdev="$(losetup -f | cut -d ' ' -f 1)""#,
        "`losetup -f` answers `/dev/loop3 (lost)` when the node it names does not exist, which is \
         the very case the helper is there to repair, and the suffix then reaches mknod as the \
         minor number",
    ),
    (
        r#"if [ ! -b "/dev/$partition" ]; then"#,
        r#"if rm -f "/dev/$partition"; then"#,
        "a partition node the container's `/dev` inherited from the host names whatever the \
         kernel had given that minor to when the container started, and keeping it means writing \
         a filesystem onto an unrelated device",
    ),
];

fn fix_loopdev_helpers(pi_gen: &Path) -> Result<()> {
    let path = pi_gen.join("scripts/common");
    let mut text =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    for (from, to, why) in LOOPDEV_EDITS {
        if text.matches(from).count() != 1 {
            bail!(
                "{} no longer holds `{from}` exactly once, so the loop device helpers cannot be \
                 corrected: {why}. Read pi-gen's helpers again before moving the pin in pi-gen.ref",
                path.display(),
            );
        }
        text = text.replace(from, to);
    }
    std::fs::write(&path, text)?;
    println!("the loop device helpers name their node and ignore the nodes the host left behind");
    Ok(())
}

fn clear_stale_container(root: &Path) {
    let existing = capture_lenient(
        "docker",
        &["ps", "-a", "--filter", "name=pigen_work", "-q"],
        root,
    )
    .unwrap_or_default();
    if existing.is_empty() {
        return;
    }
    println!("removing the container a previous build left behind");
    let _ = run("docker", ["rm", "-v", "pigen_work"], root);
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

const EMULATION_FIX: &str = "Register a statically linked emulator instead:\n  \
     Debian/Ubuntu: sudo apt install qemu-user-static binfmt-support\n  \
     Arch:          sudo pacman -S qemu-user-static-binfmt  (replaces qemu-user-binfmt; keep \
     qemu-user, pi-gen looks for qemu-aarch64 by name)\n  \
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
    if !on_path("qemu-aarch64") {
        bail!(
            "`qemu-aarch64` is not on PATH. pi-gen's build-docker.sh looks it up by that exact \
             name before it will start (`qemu-user` on most distributions).",
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
                "the binfmt_misc handler `{name}` lacks the F flag, so the container looks the \
                 interpreter up in its own filesystem and will not find it.\n{EMULATION_FIX}",
            );
        }
        let interpreter = Path::new(interpreter);
        if !is_statically_linked(interpreter)? {
            bail!(
                "the binfmt_misc handler `{name}` runs {}, which is dynamically linked. It works \
                 on this host and fails inside pi-gen's container, where its shared libraries do \
                 not exist.\n{EMULATION_FIX}",
                interpreter.display(),
            );
        }
    }
    Ok(())
}

fn build(root: &Path, board: &Board, args: &BuildArgs) -> Result<()> {
    if !cfg!(target_os = "linux") {
        bail!("pi-gen builds a Debian rootfs in a chroot and only runs on Linux");
    }
    if !on_path("docker") {
        bail!("`docker` is not on PATH; pi-gen runs in a container");
    }
    check_emulation(root)?;
    lint(root, board)?;

    let pin = pi_gen_pin(root, board)?;
    let version = format!(
        "{}-{}",
        appliance_version(root)?,
        built_on(root).replace('-', "")
    );
    println!(
        "building the {} appliance image {version} on pi-gen {}",
        board.dir, pin.git_ref,
    );

    stage_files(root, board, &version)?;
    let pi_gen = pi_gen_checkout(root, board, &pin)?;
    if args.resume {
        println!("resuming in the container the previous build left behind");
    } else {
        clear_stale_container(root);
    }

    let (user_pass, lock) = match &args.user_pass {
        None => ("locked-by-the-autd3-stage".to_owned(), "1"),
        Some(pass) => (checked_user_pass(pass)?, "0"),
    };
    let ssh = match &args.ssh_key {
        Some(path) => format!(
            "PUBKEY_SSH_FIRST_USER=\"{}\"\nPUBKEY_ONLY_SSH=1",
            ssh_pubkey(path)?,
        ),
        None => "# no SSH key was given to the build".to_owned(),
    };
    if args.ssh_key.is_none() && args.user_pass.is_none() {
        println!(
            "note: the image will have no shell access at all. The control API and reflashing the \
             card are the only ways in; pass --ssh-key to keep a way back."
        );
    }

    let config = std::fs::read_to_string(image_dir(root, board).join("config.in"))?
        .replace(
            "@IMG_NAME@",
            &format!("autd3-appliance-{}-{version}", board.dir),
        )
        .replace("@RELEASE@", &pin.release)
        .replace("@USER_PASS@", &user_pass)
        .replace("@LOCK_ACCOUNT@", lock)
        .replace("@SSH_PUBKEY@", &ssh);
    let config_path = pi_gen.join("config.autd3");
    std::fs::write(&config_path, config)?;

    let deploy = image_dir(root, board).join("deploy");
    discard_stale_images(&[pi_gen.join("deploy")])?;

    let config_arg = config_path.to_string_lossy().into_owned();
    let mut command = vec![
        "PIGEN_DOCKER_OPTS=--network host",
        "./build-docker.sh",
        "-c",
        &config_arg,
    ];
    if args.keep_container || args.resume {
        command.insert(0, "PRESERVE_CONTAINER=1");
    }
    if args.resume {
        command.insert(0, "CONTINUE=1");
    }
    run("env", command, &pi_gen)?;

    let produced = pi_gen.join("deploy");
    if newest_images(&produced)
        .context("pi-gen produced no deploy directory; the build log is the last thing it printed")?
        .is_empty()
    {
        bail!(
            "pi-gen produced no .img.xz; whatever is under {} is the previous build",
            deploy.display(),
        );
    }
    std::fs::create_dir_all(&deploy)?;
    discard_stale_images(std::slice::from_ref(&deploy))?;
    for entry in std::fs::read_dir(&produced)? {
        let from = entry?.path();
        if from.is_file() {
            copy_file(&from, &deploy.join(from.file_name().unwrap_or_default()))?;
        }
    }

    println!("images in {}", deploy.display());
    for image in newest_images(&deploy)? {
        println!("  {}", image.display());
    }
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
        "xz -dc {image} | dd of={device} bs=4M status=progress conv=fsync",
        image = shell_quote(&image),
        device = shell_quote(&args.device),
    );
    run("sudo", ["sh", "-c", &script], root)?;
    println!("written; the card is ready");
    Ok(())
}
