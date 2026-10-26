use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use protobuf::Message;
use scip::symbol::parse_symbol;
use scip::types::descriptor::Suffix;
use scip::types::symbol_information::Kind;
use scip::types::{Document, Index, SymbolInformation, SymbolRole};

use crate::util::{capture, on_path, strip_visibility};

const WORKSPACES: &[&str] = &[
    ".",
    "bindings/ffi",
    "bindings/python",
    "console",
    "simulator",
    "simulator/frontend",
    "extras/autd3-rs-emulator",
    "extras/autd3-rs-pattern-holo-wgpu",
    "firmware/cpu/board",
    "xtask",
];

const EXCLUDED_DIRS: &[&str] = &["tests", "benches", "examples"];

const GENERATED_MARKERS: &[&str] = &["AUTO-GENERATED", "DO NOT EDIT", "@generated"];

const GENERATED_MARKER_LINES: usize = 5;

const DOC_EXTENSIONS: &[&str] = &["md", "mdx", "txt"];

const SHOWN_HITS: usize = 3;

struct Definition {
    path: String,
    line: usize,
    name: String,
    qualified: String,
    owner: Option<String>,
    signature: String,
    trait_method: Option<(String, String)>,
}

#[derive(Default)]
struct Hits {
    code: Vec<(String, usize)>,
    docs: Vec<(String, usize)>,
}

#[derive(Default)]
struct Scan {
    definitions: HashMap<String, Definition>,
    referenced: HashSet<String>,
    resolved: HashSet<(String, usize, usize)>,
}

#[allow(clippy::too_many_lines)]
pub(crate) fn run_deadcode(root: &Path, reuse: bool, verbose: bool) -> Result<()> {
    fn implemented_trait_method(symbol: &str) -> Option<(String, String)> {
        let parsed = parse_symbol(symbol).ok()?;
        let descriptors = parsed.descriptors.as_slice();
        let start = descriptors.iter().position(|descriptor| {
            descriptor.name == "impl" && descriptor.suffix.enum_value_or_default() == Suffix::Type
        })?;
        match &descriptors[start + 1..] {
            [implementor, owner, method]
                if implementor.suffix.enum_value_or_default() == Suffix::TypeParameter
                    && owner.suffix.enum_value_or_default() == Suffix::TypeParameter
                    && method.suffix.enum_value_or_default() == Suffix::Method =>
            {
                Some((owner.name.clone(), method.name.clone()))
            }
            _ => None,
        }
    }
    fn qualifier(before: &str) -> Qualifier<'_> {
        let before = before.trim_end();
        let Some(receiver) = before
            .strip_suffix("::")
            .or_else(|| before.strip_suffix('.'))
        else {
            return Qualifier::None;
        };
        let name = receiver
            .rsplit(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .next()
            .unwrap_or_default();
        if name.starts_with(|c: char| c.is_ascii_uppercase()) {
            Qualifier::Type(name)
        } else {
            Qualifier::Value
        }
    }

    let out = root.join("target").join("deadcode");
    if !reuse && !on_path("rust-analyzer") {
        bail!("`rust-analyzer` is required (rustup component add rust-analyzer)");
    }
    fs::create_dir_all(&out)?;
    let mut indexes = Vec::new();
    for workspace in WORKSPACES {
        let file = out.join(format!(
            "{}.scip",
            if *workspace == "." {
                "root".to_string()
            } else {
                workspace.replace('/', "-")
            }
        ));
        if !(reuse && file.exists()) {
            println!("indexing {workspace}");
            let output = Command::new("rust-analyzer")
                .arg("scip")
                .arg(root.join(workspace))
                .arg("--output")
                .arg(&file)
                .output()
                .context("failed to spawn `rust-analyzer`")?;
            if !output.status.success() {
                bail!(
                    "`rust-analyzer scip {workspace}` exited with {}\n{}",
                    output.status,
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
        indexes.push((workspace.to_string(), file));
    }

    let mut scan = Scan::default();
    for (workspace, file) in &indexes {
        let index = Index::parse_from_bytes(
            &fs::read(file).with_context(|| format!("failed to read {}", file.display()))?,
        )
        .with_context(|| format!("failed to parse {}", file.display()))?;
        for document in &index.documents {
            let raw_path = format!("{workspace}/{}", document.relative_path);
            let mut parts: Vec<&str> = Vec::new();
            for component in raw_path.split(['/', '\\']) {
                match component {
                    "" | "." => {}
                    ".." => {
                        parts.pop();
                    }
                    _ => parts.push(component),
                }
            }
            let path = parts.join("/");
            let source = fs::read_to_string(root.join(&path)).ok();
            scan.document(&path, document, source.as_deref());
        }
    }

    let implemented: HashSet<(String, String)> = scan
        .referenced
        .iter()
        .filter_map(|symbol| implemented_trait_method(symbol))
        .collect();
    let mut candidates: Vec<Definition> = scan
        .definitions
        .into_iter()
        .filter(|(symbol, definition)| {
            !scan.referenced.contains(symbol)
                && definition
                    .trait_method
                    .as_ref()
                    .is_none_or(|method| !implemented.contains(method))
        })
        .map(|(_, definition)| definition)
        .collect();
    candidates.sort_by(|a, b| (&a.path, a.line).cmp(&(&b.path, b.line)));

    let mut by_name: HashMap<&str, Vec<usize>> = HashMap::new();
    for (index, candidate) in candidates.iter().enumerate() {
        by_name.entry(&candidate.name).or_default().push(index);
    }
    let mut hits: Vec<Hits> = candidates.iter().map(|_| Hits::default()).collect();
    let files = capture("git", &["ls-files"], root)?;
    for path in files.lines() {
        let Ok(text) = fs::read_to_string(root.join(path)) else {
            continue;
        };
        let extension = Path::new(path)
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default();
        let rust = is_rust(path);
        let doc_file = DOC_EXTENSIONS.contains(&extension) || path.starts_with("doc/");
        for (line, content) in text.lines().enumerate() {
            let comment = content.trim_start().starts_with("//");
            let bytes = content.as_bytes();
            let mut position = 0;
            let identifiers = std::iter::from_fn(move || {
                while position < bytes.len() {
                    let start = position;
                    if bytes[start].is_ascii_alphabetic() || bytes[start] == b'_' {
                        while position < bytes.len()
                            && (bytes[position].is_ascii_alphanumeric() || bytes[position] == b'_')
                        {
                            position += 1;
                        }
                        return Some((start, &content[start..position]));
                    }
                    position += if bytes[start].is_ascii_digit() {
                        bytes[start..]
                            .iter()
                            .take_while(|byte| byte.is_ascii_alphanumeric() || **byte == b'_')
                            .count()
                    } else {
                        1
                    };
                }
                None
            });
            for (column, token) in identifiers {
                let Some(indices) = by_name.get(token) else {
                    continue;
                };
                if rust && scan.resolved.contains(&(path.to_string(), line, column)) {
                    continue;
                }
                let qualifier = qualifier(&content[..column]);
                for &index in indices {
                    let candidate = &candidates[index];
                    if (candidate.path == path && candidate.line == line)
                        || candidate
                            .owner
                            .as_ref()
                            .is_some_and(|owner| !qualifier.admits(owner))
                    {
                        continue;
                    }
                    let location = (path.to_string(), line);
                    if doc_file || comment {
                        hits[index].docs.push(location);
                    } else {
                        hits[index].code.push(location);
                    }
                }
            }
        }
    }
    for hit in &mut hits {
        for locations in [&mut hit.code, &mut hit.docs] {
            locations.sort_by_key(|(path, line)| (!is_rust(path), path.clone(), *line));
        }
    }
    {
        let unmatched: Vec<usize> = (0..candidates.len())
            .filter(|&index| hits[index].code.is_empty() && hits[index].docs.is_empty())
            .collect();
        let docs_only: Vec<usize> = (0..candidates.len())
            .filter(|&index| hits[index].code.is_empty() && !hits[index].docs.is_empty())
            .collect();
        let in_code: Vec<usize> = (0..candidates.len())
            .filter(|&index| !hits[index].code.is_empty())
            .collect();
        println!();
        println!(
            "{} public items have no reference in any indexed workspace",
            candidates.len()
        );
        println!();
        println!("== no grep hits ({})", unmatched.len());
        for &index in &unmatched {
            print_candidate(&candidates[index]);
        }
        println!();
        println!(
            "== mentioned only in docs or comments ({})",
            docs_only.len()
        );
        for &index in &docs_only {
            print_candidate(&candidates[index]);
            print_hits(&hits[index].docs, verbose);
        }
        println!();
        println!(
            "== name appears in code outside the index ({})",
            in_code.len()
        );
        for &index in &in_code {
            print_candidate(&candidates[index]);
            print_hits(&hits[index].code, verbose);
        }
    };
    Ok(())
}

impl Scan {
    #[allow(clippy::too_many_lines)]
    fn document(&mut self, path: &str, document: &Document, source: Option<&str>) {
        fn public_signature(info: &SymbolInformation) -> Option<String> {
            if matches!(
                info.kind.enum_value_or_default(),
                Kind::Module
                    | Kind::Namespace
                    | Kind::Parameter
                    | Kind::SelfParameter
                    | Kind::TypeParameter
                    | Kind::Variable
            ) {
                return None;
            }
            let signature = info.signature_documentation.as_ref()?.text.trim();
            let variant = info.kind.enum_value_or_default() == Kind::EnumMember;
            ((variant || signature.starts_with("pub ")) && !signature.contains("extern \"C\""))
                .then(|| signature.lines().next().unwrap_or_default().to_string())
        }

        fn trait_method(symbol: &str) -> Option<(String, String)> {
            let parsed = parse_symbol(symbol).ok()?;
            match parsed.descriptors.as_slice() {
                [.., owner, method]
                    if owner.suffix.enum_value_or_default() == Suffix::Type
                        && method.suffix.enum_value_or_default() == Suffix::Method =>
                {
                    Some((owner.name.clone(), method.name.clone()))
                }
                _ => None,
            }
        }

        fn qualified_name(symbol: &str) -> Option<(String, Option<String>)> {
            let parsed = parse_symbol(symbol).ok()?;
            let mut parts = Vec::new();
            let mut descriptors = parsed.descriptors.iter().peekable();
            while let Some(descriptor) = descriptors.next() {
                match descriptor.suffix.enum_value_or_default() {
                    Suffix::Namespace if descriptor.name == "tests" => return None,
                    Suffix::Namespace | Suffix::Parameter | Suffix::Meta => {}
                    Suffix::Type if descriptor.name == "impl" => {
                        let mut types = Vec::new();
                        while let Some(next) = descriptors.next_if(|next| {
                            next.suffix.enum_value_or_default() == Suffix::TypeParameter
                        }) {
                            types.push(next.name.clone());
                        }
                        if types.len() != 1 {
                            return None;
                        }
                        parts.extend(types);
                    }
                    _ => parts.push(descriptor.name.clone()),
                }
            }
            let package = parsed.package.as_ref().map_or("", |package| &package.name);
            let owner = parts.len().checked_sub(2).map(|index| {
                parts[index]
                    .split('<')
                    .next()
                    .unwrap_or_default()
                    .to_string()
            });
            Some((format!("{package} {}", parts.join("::")), owner))
        }

        let mut imports = HashSet::new();
        let mut open = false;
        for (index, line) in source.unwrap_or_default().lines().enumerate() {
            if !open {
                open = strip_visibility(line.trim_start()).starts_with("use ");
            }
            if open {
                imports.insert(index);
                open = !line.contains(';');
            }
        }
        let mut lines = HashMap::new();
        for occurrence in &document.occurrences {
            let position = |index: usize| {
                occurrence
                    .range
                    .get(index)
                    .and_then(|&value| usize::try_from(value).ok())
            };
            let (Some(line), Some(column)) = (position(0), position(1)) else {
                continue;
            };
            self.resolved.insert((path.to_string(), line, column));
            if occurrence.symbol.starts_with("local ") {
                continue;
            }
            if occurrence.symbol_roles & SymbolRole::Definition as i32 != 0 {
                lines.insert(occurrence.symbol.as_str(), line);
            } else if !imports.contains(&line) {
                self.referenced.insert(occurrence.symbol.clone());
            }
        }

        if path.ends_with("/build.rs")
            || path
                .split('/')
                .any(|component| EXCLUDED_DIRS.contains(&component))
            || source.is_some_and(|source| {
                source
                    .lines()
                    .take(GENERATED_MARKER_LINES)
                    .any(|line| GENERATED_MARKERS.iter().any(|marker| line.contains(marker)))
            })
        {
            return;
        }
        let source_lines: Vec<&str> = source
            .map(|source| source.lines().collect())
            .unwrap_or_default();
        for info in &document.symbols {
            let Some(&line) = lines.get(info.symbol.as_str()) else {
                continue;
            };
            let Some(signature) = public_signature(info) else {
                continue;
            };
            let definition_line = source_lines.get(line).copied().unwrap_or_default();
            let macro_invocation = definition_line.match_indices('!').any(|(index, _)| {
                definition_line[..index]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
                    && definition_line[index + 1..]
                        .trim_start()
                        .starts_with(['(', '{', '['])
            });
            if macro_invocation
                || (info.kind.enum_value_or_default() == Kind::EnumMember
                    && definition_line.contains("#[from]"))
            {
                continue;
            }
            let Some((qualified, owner)) = qualified_name(&info.symbol) else {
                continue;
            };
            self.definitions
                .entry(info.symbol.clone())
                .or_insert_with(|| Definition {
                    path: path.to_string(),
                    line,
                    name: info.display_name.clone(),
                    qualified,
                    owner,
                    signature,
                    trait_method: (info.kind.enum_value_or_default() == Kind::TraitMethod)
                        .then(|| trait_method(&info.symbol))
                        .flatten(),
                });
        }
    }
}

enum Qualifier<'a> {
    None,
    Value,
    Type(&'a str),
}

impl Qualifier<'_> {
    fn admits(&self, owner: &str) -> bool {
        match self {
            Qualifier::None => false,
            Qualifier::Value => true,
            Qualifier::Type(name) => *name == owner || *name == "Self",
        }
    }
}

fn is_rust(path: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("rs"))
}

fn print_candidate(candidate: &Definition) {
    println!(
        "{}:{}  {}  {}",
        candidate.path,
        candidate.line + 1,
        candidate.qualified,
        candidate.signature
    );
}

fn print_hits(hits: &[(String, usize)], verbose: bool) {
    let shown = if verbose { hits.len() } else { SHOWN_HITS };
    for (path, line) in hits.iter().take(shown) {
        println!("    {path}:{}", line + 1);
    }
    if hits.len() > shown {
        println!("    ... {} more", hits.len() - shown);
    }
}
