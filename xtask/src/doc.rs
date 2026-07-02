use std::fs;
use std::path::Path;
use std::process::{Child, Command};
use std::thread::sleep;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, value};

use crate::py::{MIT_WHEELS, develop, ensure_venv, pip_install, venv_python};
use crate::util::{on_path, run};

const EXPECT_ERROR_MARKER: &str = "# xtask:expect-error";
const LONG_RUNNING_MARKER: &str = "# xtask:long-running";
const SAMPLE_TIMEOUT: Duration = Duration::from_secs(30);
const LONG_RUNNING_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Subcommand)]
pub enum DocCmd {
    Build,
    Serve {
        #[arg(long)]
        open: bool,
    },
    Samples {
        /// Only detect drift in the example list (no rewrite, no build).
        #[arg(long)]
        check: bool,
        /// Only run the Python samples (skip the Rust compile).
        #[arg(long)]
        python: bool,
    },
    Check,
    /// Inline a version snapshot's code examples to drop its `@codes` dependency.
    FreezeVersion {
        /// Target version slug (e.g. 0.1.x).
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
        } => run_python_samples(root, &doc),
        DocCmd::Samples {
            check: false,
            python: false,
        } => {
            build_samples(&samples)?;
            run_python_samples(root, &doc)
        }
        DocCmd::Build => {
            build_samples(&samples)?;
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
            )
        }
    }
}

fn build_samples(samples: &Path) -> Result<()> {
    sync_examples(samples, false)?;
    run("cargo", ["build", "--examples"], samples)
}

fn run_python_samples(root: &Path, doc: &Path) -> Result<()> {
    let bindings = root.join("bindings").join("python");
    let venv = ensure_venv(&bindings)?;
    develop(&bindings, &venv, MIT_WHEELS, false)?;
    pip_install(&bindings, &venv, &["numpy"])?;

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
    run("npm", args.iter().copied(), doc)
}
