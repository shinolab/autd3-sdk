use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::thread::sleep;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use clap::Subcommand;
use toml_edit::{value, ArrayOfTables, DocumentMut, Item, Table};

use crate::py::{develop, ensure_venv, pip_install, venv_python, MIT_WHEELS};
use crate::util::{capture, on_path, run, run_tool};

const FIRMWARE_MARKER: &str = "Firmware v";
const EXPECT_ERROR_MARKER: &str = "# xtask:expect-error";
const LONG_RUNNING_MARKER: &str = "# xtask:long-running";
const SAMPLE_TIMEOUT: Duration = Duration::from_secs(30);
const LONG_RUNNING_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Subcommand)]
pub enum DocCmd {
    /// Build the static site into `doc/dist/`
    Build {
        /// Skip compiling the Rust samples
        #[arg(long)]
        no_samples: bool,
    },
    /// Run the Astro dev server
    Serve {
        /// Open the site in a browser
        #[arg(long)]
        open: bool,
    },
    /// Compile the Rust and C# samples and run the Python ones
    Samples {
        /// Only detect drift in the example list (no rewrite, no build)
        #[arg(long)]
        check: bool,
        /// Only run the Python samples
        #[arg(long)]
        python: bool,
        /// Only compile the C# samples
        #[arg(long)]
        csharp: bool,
    },
    /// Type-check the site without building it
    Check,
    /// Inline a version snapshot's code examples to drop its `@codes` dependency
    FreezeVersion {
        /// Target version slug (e.g. 0.1.x)
        slug: String,
    },
}

pub fn run_doc(root: &Path, cmd: &DocCmd) -> Result<()> {
    let doc = root.join("doc");
    let samples = doc.join("codes").join("rust");
    match cmd {
        DocCmd::Samples { check: true, .. } => sync_examples(&samples, true),
        DocCmd::Samples {
            check: false,
            python: true,
            csharp: false,
        } => run_python_samples(root, &doc),
        DocCmd::Samples {
            check: false,
            python: false,
            csharp: true,
        } => build_csharp_samples(&doc),
        DocCmd::Samples {
            check: false,
            python,
            csharp,
        } => {
            if !*python && !*csharp {
                build_samples(&samples)?;
                run_python_samples(root, &doc)?;
                return build_csharp_samples(&doc);
            }
            if *python {
                run_python_samples(root, &doc)?;
            }
            if *csharp {
                build_csharp_samples(&doc)?;
            }
            Ok(())
        }
        DocCmd::Build { no_samples } => {
            verify_frozen_versions(&doc)?;
            verify_firmware_series(root, &doc)?;
            if !*no_samples {
                build_samples(&samples)?;
            }
            npm_install(&doc)?;
            npm(&doc, &["run", "build"])
        }
        DocCmd::Serve { open } => {
            npm_install(&doc)?;
            let mut args = vec!["run", "dev"];
            if *open {
                args.extend_from_slice(&["--", "--open"]);
            }
            npm(&doc, &args)
        }
        DocCmd::Check => {
            verify_frozen_versions(&doc)?;
            verify_firmware_series(root, &doc)?;
            npm_install(&doc)?;
            npm(&doc, &["run", "check"])
        }
        DocCmd::FreezeVersion { slug } => {
            if !on_path("node") {
                bail!("`node` is required for `doc freeze-version`");
            }
            run(
                "node",
                ["scripts/freeze-version-codes.mjs", slug.as_str()],
                &doc,
            )?;
            track_frozen_version(&doc, slug)
        }
    }
}

pub fn add_version(root: &Path, slug: &str) -> Result<bool> {
    let doc = root.join("doc");
    if version_slugs(&doc)?.iter().any(|s| s == slug) {
        println!("doc: version {slug} is already declared in astro.config.mjs");
        return Ok(false);
    }
    if !on_path("node") {
        bail!("`node` is required to write the version config");
    }
    declare_version_slug(&doc, slug)?;
    run("node", ["scripts/version-config.mjs", slug], &doc)?;
    edit_gitignore(&doc, slug, |lines, slug| {
        unignore_version_config(lines, slug);
    })?;
    Ok(true)
}

fn declare_version_slug(doc: &Path, slug: &str) -> Result<()> {
    let path = doc.join("astro.config.mjs");
    let text =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let key = text
        .find("versions:")
        .context("`versions:` not found in astro.config.mjs")?;
    let open = key
        + text[key..]
            .find('[')
            .context("`versions:` is not followed by an array in astro.config.mjs")?;
    let rest = &text[open + 1..];
    let entry = if rest.trim_start().starts_with(']') {
        format!("{{ slug: \"{slug}\" }}")
    } else {
        format!("{{ slug: \"{slug}\" }}, ")
    };
    let new_text = format!("{}{entry}{rest}", &text[..=open]);
    fs::write(&path, new_text).with_context(|| format!("failed to write {}", path.display()))?;
    println!("doc: declared version {slug} in {}", path.display());
    Ok(())
}

fn unignore_version_config(lines: &mut Vec<String>, slug: &str) {
    for line in &mut *lines {
        if line.trim() == "src/content/versions/" {
            *line = "src/content/versions/*".to_string();
        }
    }
    push_unique(lines, format!("!src/content/versions/{slug}.json"));
}

fn push_unique(lines: &mut Vec<String>, line: String) {
    if !lines.iter().any(|l| l.trim() == line.as_str()) {
        lines.push(line);
    }
}

fn edit_gitignore(
    doc: &Path,
    slug: &str,
    edit: impl FnOnce(&mut Vec<String>, &str),
) -> Result<bool> {
    let path = doc.join(".gitignore");
    let text =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    edit(&mut lines, slug);

    let mut new_text = lines.join("\n");
    if text.ends_with('\n') {
        new_text.push('\n');
    }
    if new_text == text {
        println!("doc: {slug} already tracked in {}", path.display());
        return Ok(false);
    }
    fs::write(&path, new_text).with_context(|| format!("failed to write {}", path.display()))?;
    println!("doc: tracked {slug} in {}", path.display());
    Ok(true)
}

fn track_frozen_version(doc: &Path, slug: &str) -> Result<()> {
    edit_gitignore(doc, slug, |lines, slug| {
        let ignore_root = format!("src/content/docs/{slug}/");
        let ignore_locale = format!("src/content/docs/*/{slug}/");
        lines.retain(|l| {
            let t = l.trim();
            t != ignore_root && t != ignore_locale
        });
        unignore_version_config(lines, slug);
        push_unique(lines, format!("!{ignore_root}"));
        push_unique(lines, format!("!{ignore_locale}"));
    })
    .map(|_| ())
}

fn version_slugs(doc: &Path) -> Result<Vec<String>> {
    let cfg = fs::read_to_string(doc.join("astro.config.mjs"))
        .context("failed to read astro.config.mjs")?;
    let mut slugs = Vec::new();
    let mut rest = cfg.as_str();
    while let Some(pos) = rest.find("slug:") {
        rest = &rest[pos + "slug:".len()..];
        let Some(q) = rest.find(['"', '\'']) else {
            break;
        };
        let quote = rest[q..].chars().next().unwrap();
        let after = &rest[q + 1..];
        let Some(end) = after.find(quote) else {
            break;
        };
        slugs.push(after[..end].to_string());
        rest = &after[end + 1..];
    }
    Ok(slugs)
}

fn live_pages(dir: &Path, slugs: &[String], out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        if path.is_dir() {
            if !slugs.contains(&name) {
                live_pages(&path, slugs, out)?;
            }
        } else if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("md" | "mdx")
        ) {
            out.push(path);
        }
    }
    Ok(())
}

fn firmware_series_spans(text: &str) -> Vec<(usize, usize, String)> {
    let mut spans = Vec::new();
    let mut base = 0;
    while let Some(pos) = text[base..].find(FIRMWARE_MARKER) {
        let start = base + pos + FIRMWARE_MARKER.len();
        let len = text[start..]
            .find(|c: char| !c.is_ascii_digit() && c != '.' && c != 'x')
            .unwrap_or(text.len() - start);
        base = start + len;
        let Some(series) = text[start..base].strip_suffix(".x") else {
            continue;
        };
        if series.split('.').count() == 2 && series.bytes().all(|b| b.is_ascii_digit() || b == b'.')
        {
            spans.push((start, base, series.to_string()));
        }
    }
    spans
}

pub fn rewrite_firmware_series(root: &Path, series: &str) -> Result<usize> {
    let doc = root.join("doc");
    let slugs = version_slugs(&doc)?;
    let mut pages = Vec::new();
    live_pages(&doc.join("src/content/docs"), &slugs, &mut pages)?;

    let mut count = 0;
    for page in pages {
        let text =
            fs::read_to_string(&page).with_context(|| format!("reading {}", page.display()))?;
        let spans = firmware_series_spans(&text);
        if spans.is_empty() {
            continue;
        }
        let mut new = String::with_capacity(text.len());
        let mut cursor = 0;
        for (start, end, _) in spans {
            new.push_str(&text[cursor..start]);
            new.push_str(series);
            new.push_str(".x");
            cursor = end;
        }
        new.push_str(&text[cursor..]);
        if new != text {
            fs::write(&page, new).with_context(|| format!("writing {}", page.display()))?;
        }
        count += 1;
    }
    Ok(count)
}

fn verify_firmware_series(root: &Path, doc: &Path) -> Result<()> {
    let expected = crate::bump::firmware_series(root)?;
    let slugs = version_slugs(doc)?;
    let mut pages = Vec::new();
    live_pages(&doc.join("src/content/docs"), &slugs, &mut pages)?;

    let mut offenders = Vec::new();
    let mut found = 0;
    for page in pages {
        let text =
            fs::read_to_string(&page).with_context(|| format!("reading {}", page.display()))?;
        for (_, _, series) in firmware_series_spans(&text) {
            found += 1;
            if series != expected {
                offenders.push(format!("{}: {FIRMWARE_MARKER}{series}.x", page.display()));
            }
        }
    }
    if found == 0 {
        bail!(
            "no `{FIRMWARE_MARKER}<major>.<minor>.x` marker found in the current docs; the supported \
             firmware version must be stated (and kept in sync with firmware/cpu/fw/src/version.rs)"
        );
    }
    if !offenders.is_empty() {
        bail!(
            "docs advertise a firmware version that firmware/cpu/fw/src/version.rs does not build \
             (expected `{FIRMWARE_MARKER}{expected}.x`):\n  {}\n\
             run `cargo xtask bump-version firmware <version>` (it rewrites these pages), or fix the marker by hand. \
             frozen version snapshots are exempt: they record the firmware version of their own SDK release.",
            offenders.join("\n  ")
        );
    }
    Ok(())
}

fn verify_frozen_versions(doc: &Path) -> Result<()> {
    if !on_path("git") {
        return Ok(());
    }
    let slugs = version_slugs(doc)?;
    if slugs.is_empty() {
        return Ok(());
    }
    let mut missing_config = Vec::new();
    for slug in &slugs {
        let rel = format!("src/content/versions/{slug}.json");
        let tracked = capture("git", &["ls-files", "--error-unmatch", &rel], doc).is_ok();
        if !tracked {
            missing_config.push(rel);
        }
    }
    if !missing_config.is_empty() {
        bail!(
            "declared version(s) have no committed config file (they are `.gitignore`d):\n  {}\n\
             these exist locally but are absent on a clean checkout, so CI's `astro build` fails to \
             read the version config. run `cargo xtask doc freeze-version <slug>` and commit the file.",
            missing_config.join("\n  ")
        );
    }
    let Ok(tracked) = capture("git", &["ls-files", "src/content/docs"], doc) else {
        return Ok(());
    };
    let mut offenders = Vec::new();
    for rel in tracked.lines() {
        let ext = Path::new(rel).extension().and_then(|e| e.to_str());
        if !matches!(ext, Some("md" | "mdx")) {
            continue;
        }
        let Some(slug) = slugs
            .iter()
            .find(|s| rel.split('/').any(|seg| seg == s.as_str()))
        else {
            continue;
        };
        let text =
            fs::read_to_string(doc.join(rel)).with_context(|| format!("failed to read {rel}"))?;
        if (text.contains("@codes/") && text.contains("?raw")) || text.contains("excerpt(") {
            offenders.push(format!("{rel} (version {slug})"));
        }
    }
    if !offenders.is_empty() {
        bail!(
            "committed version snapshot(s) still depend on live `@codes` sources (not frozen):\n  {}\n\
             run `cargo xtask doc freeze-version <slug>` for each affected version, then re-commit.",
            offenders.join("\n  ")
        );
    }
    Ok(())
}

fn build_samples(samples: &Path) -> Result<()> {
    sync_examples(samples, false)?;
    run("cargo", ["build", "--examples"], samples)
}

fn build_csharp_samples(doc: &Path) -> Result<()> {
    if !on_path("dotnet") {
        bail!("`dotnet` is required for the C# doc samples (install the .NET SDK)");
    }
    let csproj = doc
        .join("codes")
        .join("csharp")
        .join("AUTD3.DocSamples.csproj");
    if !csproj.is_file() {
        bail!("C# doc sample project not found: {}", csproj.display());
    }
    run(
        "dotnet",
        [
            "build",
            "codes/csharp/AUTD3.DocSamples.csproj",
            "-c",
            "Release",
        ],
        doc,
    )?;
    println!("doc: C# samples compiled");
    Ok(())
}

fn run_python_samples(root: &Path, doc: &Path) -> Result<()> {
    let bindings = root.join("bindings").join("python");
    let venv = ensure_venv(&bindings)?;
    develop(&bindings, &venv, MIT_WHEELS, false)?;
    pip_install(&bindings, &venv, &["numpy", "scipy", "polars"])?;

    let py_codes = doc.join("codes").join("python");
    let runner = py_codes.join("scripts").join("run_sample.py");
    if !runner.is_file() {
        bail!("python sample runner not found: {}", runner.display());
    }
    let examples_dir = py_codes.join("examples");
    let mut rels = Vec::new();
    collect_py(&examples_dir, &examples_dir, &mut rels)?;
    rels.sort();

    let python = venv_python(&venv);
    let mut failures = Vec::new();
    for rel in &rels {
        let script = examples_dir.join(rel);
        let text = fs::read_to_string(&script)
            .with_context(|| format!("failed to read {}", script.display()))?;
        let expect_error = text.contains(EXPECT_ERROR_MARKER);
        let long_running = text.contains(LONG_RUNNING_MARKER);
        let limit = if long_running {
            LONG_RUNNING_TIMEOUT
        } else {
            SAMPLE_TIMEOUT
        };

        let mut child = Command::new(&python)
            .arg("-B")
            .arg(&runner)
            .arg(&script)
            .current_dir(&py_codes)
            .spawn()
            .with_context(|| format!("failed to spawn python for {}", script.display()))?;

        let reason = match wait_timeout(&mut child, limit)? {
            Some(status) if long_running => {
                (!status.success()).then(|| format!("exited early with {status}"))
            }
            Some(status) => {
                let ok = status.success();
                (ok == expect_error).then(|| {
                    let want = if expect_error { "non-zero" } else { "success" };
                    format!("expected {want}, got {status}")
                })
            }
            None if long_running => None,
            None => Some(format!("timed out after {}s", limit.as_secs())),
        };

        if let Some(reason) = reason {
            failures.push(format!("  {rel} ({reason})"));
            println!("doc: python sample FAILED: {rel} ({reason})");
        } else {
            println!("doc: python sample ok: {rel}");
        }
    }
    if !failures.is_empty() {
        bail!(
            "{} python sample(s) failed:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
    println!("doc: {} python samples passed", rels.len());
    Ok(())
}

fn wait_timeout(child: &mut Child, limit: Duration) -> Result<Option<std::process::ExitStatus>> {
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().context("failed to poll python child")? {
            return Ok(Some(status));
        }
        if start.elapsed() >= limit {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(None);
        }
        sleep(Duration::from_millis(100));
    }
}

fn collect_py(dir: &Path, base: &Path, out: &mut Vec<String>) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "__pycache__") {
                continue;
            }
            collect_py(&path, base, out)?;
        } else if path.extension().is_some_and(|e| e == "py") {
            let rel = path
                .strip_prefix(base)
                .with_context(|| format!("{} is not under {}", path.display(), base.display()))?;
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

fn sync_examples(samples: &Path, check: bool) -> Result<()> {
    let examples_dir = samples.join("examples");
    let mut rels = Vec::new();
    collect_rs(&examples_dir, &examples_dir, &mut rels)?;
    rels.sort();

    let manifest_path = samples.join("Cargo.toml");
    let text = fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let mut doc = text
        .parse::<DocumentMut>()
        .with_context(|| format!("failed to parse {}", manifest_path.display()))?;

    let mut tables = ArrayOfTables::new();
    for rel in &rels {
        let name = rel
            .strip_suffix(".rs")
            .unwrap_or(rel)
            .replace(['/', '-'], "_");
        let mut t = Table::new();
        t.decor_mut().set_prefix("\n");
        t["name"] = value(name);
        t["path"] = value(format!("examples/{rel}"));
        tables.push(t);
    }
    doc["example"] = Item::ArrayOfTables(tables);

    let new_text = doc.to_string();
    if new_text == text {
        return Ok(());
    }
    if check {
        bail!(
            "example list in {} is out of sync with the filesystem; run `cargo xtask doc samples`",
            manifest_path.display()
        );
    }
    fs::write(&manifest_path, new_text)
        .with_context(|| format!("failed to write {}", manifest_path.display()))?;
    println!(
        "doc: synced {} example entries in {}",
        rels.len(),
        manifest_path.display()
    );
    Ok(())
}

fn collect_rs(dir: &Path, base: &Path, out: &mut Vec<String>) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            collect_rs(&path, base, out)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            let rel = path
                .strip_prefix(base)
                .with_context(|| format!("{} is not under {}", path.display(), base.display()))?;
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

fn npm_install(doc: &Path) -> Result<()> {
    if !on_path("npm") {
        bail!("`npm` is required for the `doc` scope (install Node.js)");
    }
    if doc.join("package-lock.json").is_file() {
        npm(doc, &["ci"])
    } else {
        npm(doc, &["install"])
    }
}

fn npm(doc: &Path, args: &[&str]) -> Result<()> {
    if !on_path("npm") {
        bail!("`npm` is required for the `doc` scope (install Node.js)");
    }
    run_tool("npm", args.iter().copied(), doc)
}
