use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Args;

const WALK_SKIP: &[&str] = &[".git", "target", "node_modules", ".venv"];

#[derive(Args, Clone, Copy)]
pub struct CleanArgs {
    #[arg(
        long,
        short = 'n',
        help = "List what would be removed without removing it"
    )]
    pub dry_run: bool,
    #[arg(
        long,
        help = "Also remove the fetched dependency trees (node_modules, .venv)"
    )]
    pub deps: bool,
}

pub struct Cleaner<'a> {
    root: &'a Path,
    args: CleanArgs,
    removed: usize,
    freed: u64,
}

impl<'a> Cleaner<'a> {
    pub fn new(root: &'a Path, args: CleanArgs) -> Self {
        Self {
            root,
            args,
            removed: 0,
            freed: 0,
        }
    }

    pub fn root(&self) -> &Path {
        self.root
    }

    pub fn remove(&mut self, path: &Path) -> Result<()> {
        let Ok(meta) = std::fs::symlink_metadata(path) else {
            return Ok(());
        };
        let dir = meta.is_dir();
        let size = if dir { dir_size(path) } else { meta.len() };
        let shown = path.strip_prefix(self.root).unwrap_or(path);
        if self.args.dry_run {
            println!("would remove {}", shown.display());
        } else {
            if dir {
                std::fs::remove_dir_all(path)
            } else {
                std::fs::remove_file(path)
            }
            .with_context(|| format!("removing {}", path.display()))?;
            println!("removed {}", shown.display());
        }
        self.removed += 1;
        self.freed += size;
        Ok(())
    }

    pub fn path(&mut self, rel: &str) -> Result<()> {
        let path = self.root.join(rel);
        self.remove(&path)
    }

    pub fn paths(&mut self, rels: &[&str]) -> Result<()> {
        rels.iter().try_for_each(|rel| self.path(rel))
    }

    pub fn deps(&mut self, rel: &str) -> Result<()> {
        if self.args.deps {
            self.path(rel)?;
        }
        Ok(())
    }

    pub fn children(&mut self, rel: &str, keep: &[&str]) -> Result<()> {
        for path in sorted_entries(&self.root.join(rel)) {
            if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| keep.contains(&name))
            {
                continue;
            }
            self.remove(&path)?;
        }
        Ok(())
    }

    pub fn subdirs(&self, rel: &str) -> Vec<PathBuf> {
        sorted_entries(&self.root.join(rel))
            .into_iter()
            .filter(|path| path.is_dir())
            .collect()
    }

    pub fn in_each_subdir(&mut self, rel: &str, names: &[&str]) -> Result<()> {
        for dir in self.subdirs(rel) {
            for name in names {
                let path = dir.join(name);
                self.remove(&path)?;
            }
        }
        Ok(())
    }

    pub fn matching_in_each_subdir(
        &mut self,
        rel: &str,
        matches: impl Fn(&str) -> bool,
    ) -> Result<()> {
        for dir in self.subdirs(rel) {
            for path in sorted_entries(&dir) {
                if path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(&matches)
                {
                    self.remove(&path)?;
                }
            }
        }
        Ok(())
    }

    pub fn nested(&mut self, rel: &str, names: &[&str]) -> Result<()> {
        let mut found = Vec::new();
        collect_nested(&self.root.join(rel), names, &mut found);
        for path in found {
            self.remove(&path)?;
        }
        Ok(())
    }

    pub fn finish(self) {
        if self.removed == 0 {
            println!("nothing to clean");
            return;
        }
        let verb = if self.args.dry_run {
            "would free"
        } else {
            "freed"
        };
        println!(
            "{} {} entries, {verb} {}",
            if self.args.dry_run {
                "would remove"
            } else {
                "removed"
            },
            self.removed,
            human_size(self.freed)
        );
    }
}

pub fn scope(
    root: &Path,
    args: CleanArgs,
    clean: impl FnOnce(&mut Cleaner) -> Result<()>,
) -> Result<()> {
    let mut cleaner = Cleaner::new(root, args);
    clean(&mut cleaner)?;
    cleaner.finish();
    Ok(())
}

fn sorted_entries(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    paths.sort();
    paths
}

fn collect_nested(dir: &Path, names: &[&str], out: &mut Vec<PathBuf>) {
    for path in sorted_entries(dir) {
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if names.contains(&name) {
            out.push(path);
            continue;
        }
        if path.is_dir() && !WALK_SKIP.contains(&name) {
            collect_nested(&path, names, out);
        }
    }
}

fn dir_size(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| match entry.file_type() {
            Ok(kind) if kind.is_dir() => dir_size(&entry.path()),
            Ok(kind) if kind.is_file() => entry.metadata().map_or(0, |meta| meta.len()),
            _ => 0,
        })
        .sum()
}

fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut scale = 1u64;
    let mut unit = 0;
    while unit + 1 < UNITS.len() && bytes / scale >= 1024 {
        scale *= 1024;
        unit += 1;
    }
    if unit == 0 {
        return format!("{bytes} B");
    }
    let tenths = bytes * 10 / scale;
    format!("{}.{} {}", tenths / 10, tenths % 10, UNITS[unit])
}
