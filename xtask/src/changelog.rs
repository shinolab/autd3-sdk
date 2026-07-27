use std::collections::HashSet;
use std::path::Path;

use anyhow::{Context, Result, bail};
use clap::Args;

use crate::component::{COMPONENTS, Component, detect, release_sections};
use crate::util::{capture, capture_lenient};

#[derive(Args)]
pub struct ChangelogCmd {
    /// Tag to write the release notes for (e.g. v1.2.3). Required with `--release-notes`
    #[arg(long)]
    tag: Option<String>,
    /// Write the notes of a single release instead of the whole CHANGELOG.md
    #[arg(long)]
    release_notes: bool,
    /// File to write to (CHANGELOG.md by default; stdout for `--release-notes`)
    #[arg(short, long)]
    output: Option<String>,
}

pub fn run_changelog(root: &Path, cmd: &ChangelogCmd) -> Result<()> {
    if cmd.release_notes {
        let tag = cmd
            .tag
            .as_deref()
            .context("--release-notes requires --tag")?;
        write_release_notes(root, tag, cmd.output.as_deref())
    } else {
        let output = cmd.output.clone().unwrap_or_else(|| "CHANGELOG.md".into());
        write_changelog_file(root, &output)
    }
}

fn scope_args(args: &mut Vec<String>, component: &Component, root: &Path) -> Result<()> {
    args.push("--tag-pattern".into());
    args.push(component.tag_pattern());
    for sha in unrelated_commits(root, component)? {
        args.push("--skip-commit".into());
        args.push(sha);
    }
    Ok(())
}

fn unrelated_commits(root: &Path, component: &Component) -> Result<Vec<String>> {
    let all = capture("git", &["rev-list", "HEAD"], root)?;
    let specs: Vec<String> = component
        .include_paths
        .iter()
        .map(|p| format!(":(glob){p}"))
        .collect();
    let mut args: Vec<&str> = vec!["rev-list", "--full-history", "HEAD", "--"];
    args.extend(specs.iter().map(String::as_str));
    let scoped = capture("git", &args, root)?;
    let kept: HashSet<&str> = scoped.lines().collect();
    Ok(all
        .lines()
        .filter(|sha| !kept.contains(sha))
        .map(str::to_string)
        .collect())
}

fn strip_tag_prefixes(body: &str, component: &Component) -> String {
    let mut body = body.to_string();
    for prefix in component.tag_prefixes() {
        body = body.replace(&format!("## [{prefix}"), "## [");
    }
    body
}

fn write_release_notes(root: &Path, tag: &str, output: Option<&str>) -> Result<()> {
    let (primary, _) =
        detect(tag).with_context(|| format!("tag `{tag}` matches no known release component"))?;

    if capture(
        "git",
        &["rev-parse", "-q", "--verify", &format!("refs/tags/{tag}")],
        root,
    )
    .is_err()
    {
        bail!(
            "tag `{tag}` does not exist in this repository; release notes would silently describe \
             the previous release (create the tag first, or fetch tags with `fetch-depth: 0`)"
        );
    }

    let sections = release_sections(primary);
    let multi = sections.len() > 1;

    let mut doc = String::new();
    for section in sections {
        let mut args: Vec<String> = Vec::new();
        scope_args(&mut args, section, root)?;
        args.push("--tag".into());
        args.push(tag.to_string());
        args.push("--latest".into());
        args.push("--strip".into());
        args.push("header".into());

        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let body = capture_lenient("git-cliff", &refs, root)?;
        if body.is_empty() {
            continue;
        }
        if multi {
            doc.push_str("# ");
            doc.push_str(section.section);
            doc.push_str("\n\n");
        }
        doc.push_str(&strip_tag_prefixes(&body, section));
        doc.push_str("\n\n");
    }

    let doc = doc.trim_end().to_string();
    if doc.is_empty() {
        bail!(
            "no release notes generated for `{tag}`: git-cliff matched no commits for any section \
             (is the tag present with full history? `git-cliff` may have failed silently)"
        );
    }
    let doc = doc + "\n";
    match output {
        Some(out) => {
            std::fs::write(root.join(out), doc).with_context(|| format!("writing {out}"))?;
        }
        None => print!("{doc}"),
    }
    Ok(())
}

pub fn write_changelog_file(root: &Path, output: &str) -> Result<()> {
    let mut doc = String::from("# Changelog\n");
    for component in COMPONENTS {
        let mut args: Vec<String> = Vec::new();
        scope_args(&mut args, component, root)?;
        args.push("--strip".into());
        args.push("header".into());
        if let Some(tag) = component.pending_tag(root)? {
            args.push("--tag".into());
            args.push(tag);
        }
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let body = capture_lenient("git-cliff", &refs, root)?;

        doc.push_str("\n# ");
        doc.push_str(component.section);
        doc.push_str("\n\n");
        if body.is_empty() {
            doc.push_str("_No releases yet._\n");
        } else {
            doc.push_str(&strip_tag_prefixes(&body, component));
            doc.push('\n');
        }
    }

    std::fs::write(root.join(output), doc).with_context(|| format!("writing {output}"))?;
    Ok(())
}
