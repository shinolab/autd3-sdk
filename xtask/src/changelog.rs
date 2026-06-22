use std::path::Path;

use anyhow::{Context, Result};
use clap::Args;

use crate::component::{COMPONENTS, Component, detect};
use crate::util::{capture_lenient, run};

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
    let (component, _) =
        detect(tag).with_context(|| format!("tag `{tag}` matches no known release component"))?;

    let mut args: Vec<String> = Vec::new();
    scope_args(&mut args, component);
    args.push("--tag".into());
    args.push(tag.to_string());
    args.push("--latest".into());
    args.push("--strip".into());
    args.push("header".into());
    if let Some(out) = output {
        args.push("--output".into());
        args.push(out.to_string());
    }
    run("git-cliff", args.iter().map(String::as_str), root)
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
