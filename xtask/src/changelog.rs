use std::path::Path;

use anyhow::{Context, Result, bail};
use clap::Args;

use crate::component::{COMPONENTS, Component, detect, release_sections};
use crate::util::capture_lenient;

#[derive(Args)]
pub struct ChangelogCmd {
    #[arg(long)]
    tag: Option<String>,

    #[arg(long)]
    release_notes: bool,

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
        write_changelog_file(root, cmd.tag.as_deref(), &output)
    }
}

fn scope_args(args: &mut Vec<String>, component: &Component) {
    args.push("--tag-pattern".into());
    args.push(component.tag_pattern());
    for path in component.include_paths {
        args.push("--include-path".into());
        args.push((*path).to_string());
    }
}

fn write_release_notes(root: &Path, tag: &str, output: Option<&str>) -> Result<()> {
    let (primary, _) =
        detect(tag).with_context(|| format!("tag `{tag}` matches no known release component"))?;

    let sections = release_sections(primary);
    let multi = sections.len() > 1;

    let mut doc = String::new();
    for section in sections {
        let mut args: Vec<String> = Vec::new();
        args.push("--tag-pattern".into());
        args.push(section.tag_pattern());
        for path in section.include_paths {
            args.push("--include-path".into());
            args.push((*path).to_string());
        }
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
        doc.push_str(&body);
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

pub fn write_changelog_file(root: &Path, tag: Option<&str>, output: &str) -> Result<()> {
    let tagged = tag.and_then(detect).map(|(c, _)| c.name);

    let mut doc = String::from("# Changelog\n");
    for component in COMPONENTS {
        let mut args: Vec<String> = Vec::new();
        scope_args(&mut args, component);
        args.push("--strip".into());
        args.push("header".into());
        if tagged == Some(component.name)
            && let Some(tag) = tag
        {
            args.push("--tag".into());
            args.push(tag.to_string());
        }
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let body = capture_lenient("git-cliff", &refs, root)?;

        doc.push_str("\n# ");
        doc.push_str(component.section);
        doc.push_str("\n\n");
        if body.is_empty() {
            doc.push_str("_No releases yet._\n");
        } else {
            doc.push_str(&body);
            doc.push('\n');
        }
    }

    std::fs::write(root.join(output), doc).with_context(|| format!("writing {output}"))?;
    Ok(())
}
