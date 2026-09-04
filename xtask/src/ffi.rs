use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Subcommand;

use crate::util::{cargo_fmt_packages, run};

const CSHARP_SRC: &str = "bindings/csharp/src";

const CSHARP_UNUSED_EXPORTS: &[&str] = &[
    "autd3_core_sampling_config_freq_4k",
    "autd3_core_sampling_config_freq_40k",
];

const OPTION_GETTER_MARKER: &str = "_option_get_";

#[derive(Subcommand)]
pub enum FfiCmd {
    /// Build the C ABI cdylibs
    Build {
        /// Build the dev profile instead of release
        #[arg(long)]
        debug: bool,
    },
    /// Test the FFI workspace
    Test,
    /// Measure FFI workspace test coverage with cargo-llvm-cov
    Coverage {
        /// Open the HTML report in a browser
        #[arg(long)]
        open: bool,
    },
    /// Clippy the FFI workspace
    Lint,
    /// Rustfmt the FFI workspace
    Format {
        /// Rewrite the files instead of only checking them
        #[arg(long)]
        fix: bool,
    },
    #[command(about = "Compare the exported C ABI symbols with the C# DllImport declarations")]
    Drift,
}

pub fn run_ffi(root: &Path, cmd: &FfiCmd) -> Result<()> {
    let dir = root.join("bindings").join("ffi");
    match cmd {
        FfiCmd::Build { debug } => {
            let mut args = vec!["build", "--workspace"];
            if !*debug {
                args.push("--release");
            }
            run("cargo", args, &dir)
        }
        FfiCmd::Test => run("cargo", vec!["test", "--workspace"], &dir),
        FfiCmd::Coverage { open } => crate::rust::coverage(
            &dir,
            &["llvm-cov", "--no-report", "--workspace", "--lib", "--tests"],
            &[],
            *open,
        ),
        FfiCmd::Lint => {
            let mut args = vec!["clippy", "--workspace", "--all-targets"];
            args.extend(["--", "-D", "warnings"]);
            run("cargo", args, &dir)
        }
        FfiCmd::Format { fix } => cargo_fmt_packages(&dir, *fix),
        FfiCmd::Drift => drift(root, &dir),
    }
}

fn drift(root: &Path, ffi: &Path) -> Result<()> {
    let exporters = exporting_macros(ffi)?;
    let mut exported: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (lib, dir) in cdylibs(ffi)? {
        exported.insert(lib, exported_symbols(&dir, &exporters)?);
    }
    let declared = csharp_imports(&root.join(CSHARP_SRC))?;

    let mut problems = Vec::new();
    for (lib, declarations) in &declared {
        let Some(symbols) = exported.get(lib) else {
            problems.push(format!(
                "C# imports `{lib}`, which is not a cdylib in bindings/ffi"
            ));
            continue;
        };
        for symbol in declarations {
            if !symbols.contains(symbol) {
                problems.push(format!(
                    "C# declares `{symbol}` in `{lib}`, which exports no such symbol"
                ));
            }
        }
    }
    let empty = BTreeSet::new();
    for (lib, symbols) in &exported {
        let declarations = declared.get(lib).unwrap_or(&empty);
        for symbol in symbols {
            if declarations.contains(symbol) || allowed_without_csharp(symbol) {
                continue;
            }
            problems.push(format!(
                "`{lib}` exports `{symbol}`, which no C# binding declares"
            ));
        }
    }

    if problems.is_empty() {
        let total: usize = exported.values().map(BTreeSet::len).sum();
        println!(
            "ffi drift: {total} exported symbols across {} cdylibs match the C# declarations",
            exported.len()
        );
        return Ok(());
    }
    for problem in &problems {
        eprintln!("{problem}");
    }
    bail!(
        "{} binding drift(s) between bindings/ffi and C#",
        problems.len()
    )
}

fn allowed_without_csharp(symbol: &str) -> bool {
    symbol.contains(OPTION_GETTER_MARKER) || CSHARP_UNUSED_EXPORTS.contains(&symbol)
}

fn ffi_crates(ffi: &Path) -> Result<Vec<PathBuf>> {
    let mut dirs = Vec::new();
    for entry in std::fs::read_dir(ffi).with_context(|| format!("reading {}", ffi.display()))? {
        let dir = entry?.path();
        if dir.join("Cargo.toml").is_file() {
            dirs.push(dir);
        }
    }
    dirs.sort();
    Ok(dirs)
}

fn cdylibs(ffi: &Path) -> Result<Vec<(String, PathBuf)>> {
    let mut libs = Vec::new();
    for dir in ffi_crates(ffi)? {
        let manifest = dir.join("Cargo.toml");
        let text = std::fs::read_to_string(&manifest)
            .with_context(|| format!("failed to read {}", manifest.display()))?;
        let doc = text
            .parse::<toml_edit::DocumentMut>()
            .with_context(|| format!("failed to parse {}", manifest.display()))?;
        let Some(lib) = doc.get("lib") else {
            continue;
        };
        let is_cdylib = lib
            .get("crate-type")
            .and_then(toml_edit::Item::as_array)
            .is_some_and(|kinds| kinds.iter().any(|k| k.as_str() == Some("cdylib")));
        if !is_cdylib {
            continue;
        }
        let name = lib
            .get("name")
            .and_then(toml_edit::Item::as_str)
            .with_context(|| format!("no [lib] name in {}", manifest.display()))?;
        libs.push((name.to_string(), dir));
    }
    libs.sort();
    Ok(libs)
}

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            rust_sources(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
    out.sort();
    Ok(())
}

fn exporting_macros(ffi: &Path) -> Result<BTreeMap<String, BTreeSet<String>>> {
    let mut macros = BTreeMap::new();
    for dir in ffi_crates(ffi)? {
        let mut files = Vec::new();
        rust_sources(&dir.join("src"), &mut files)?;
        for file in files {
            let text = std::fs::read_to_string(&file)
                .with_context(|| format!("failed to read {}", file.display()))?;
            for (name, body) in macro_definitions(&text) {
                if !body.contains("no_mangle") {
                    continue;
                }
                let mut fixed = BTreeSet::new();
                collect_plain_exports(body, &mut fixed);
                macros.insert(name, fixed);
            }
        }
    }
    Ok(macros)
}

fn macro_definitions(text: &str) -> Vec<(String, &str)> {
    const KEY: &str = "macro_rules!";
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(offset) = text[from..].find(KEY) {
        let after = from + offset + KEY.len();
        let name_start = after + (text[after..].len() - text[after..].trim_start().len());
        let name_end = name_start
            + text[name_start..]
                .find(|c: char| !is_ident_char(c))
                .unwrap_or(0);
        let Some(brace) = text[name_end..].find('{').map(|i| name_end + i) else {
            break;
        };
        let end = matching(text, brace, b'{', b'}');
        if name_end > name_start {
            found.push((text[name_start..name_end].to_string(), &text[brace..end]));
        }
        from = end;
    }
    found
}

fn matching(text: &str, open: usize, opener: u8, closer: u8) -> usize {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    for (index, byte) in bytes[open..].iter().enumerate() {
        if *byte == opener {
            depth += 1;
        } else if *byte == closer {
            depth -= 1;
            if depth == 0 {
                return open + index;
            }
        }
    }
    text.len()
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn exported_symbols(
    dir: &Path,
    exporters: &BTreeMap<String, BTreeSet<String>>,
) -> Result<BTreeSet<String>> {
    let mut symbols = BTreeSet::new();
    let mut files = Vec::new();
    rust_sources(&dir.join("src"), &mut files)?;
    for file in files {
        let text = std::fs::read_to_string(&file)
            .with_context(|| format!("failed to read {}", file.display()))?;
        collect_plain_exports(&text, &mut symbols);
        collect_macro_exports(&text, exporters, &mut symbols);
    }
    Ok(symbols)
}

fn collect_plain_exports(text: &str, symbols: &mut BTreeSet<String>) {
    let mut tagged = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("#[") {
            tagged |= line.contains("no_mangle");
            continue;
        }
        if !tagged {
            continue;
        }
        tagged = false;
        if let Some(name) = extern_fn_name(line) {
            symbols.insert(name.to_string());
        }
    }
}

fn extern_fn_name(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("pub ")?.trim_start();
    let rest = rest.strip_prefix("unsafe ").unwrap_or(rest).trim_start();
    let rest = rest.strip_prefix("extern \"C\" fn ")?.trim_start();
    let end = rest.find(|c: char| !is_ident_char(c)).unwrap_or(rest.len());
    (end > 0).then(|| &rest[..end])
}

fn collect_macro_exports(
    text: &str,
    exporters: &BTreeMap<String, BTreeSet<String>>,
    symbols: &mut BTreeSet<String>,
) {
    let mut from = 0;
    while let Some(offset) = text[from..].find("!(") {
        let bang = from + offset;
        from = bang + 2;
        let name_start = text[..bang]
            .rfind(|c: char| !is_ident_char(c))
            .map_or(0, |i| i + 1);
        if name_start == bang {
            continue;
        }
        let Some(fixed) = exporters.get(&text[name_start..bang]) else {
            continue;
        };
        symbols.extend(fixed.iter().cloned());
        let end = matching(text, bang + 1, b'(', b')');
        for symbol in autd3_idents(&text[bang + 2..end]) {
            symbols.insert(symbol);
        }
        from = end;
    }
}

const SYMBOL_PREFIX: &str = "autd3_";

fn autd3_idents(args: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = args;
    while let Some(start) = rest.find(SYMBOL_PREFIX) {
        let preceded_by_ident =
            start > 0 && rest[..start].chars().next_back().is_some_and(is_ident_char);
        let end = start
            + rest[start..]
                .find(|c: char| !is_ident_char(c))
                .unwrap_or(rest.len() - start);
        if !preceded_by_ident && !rest[end..].starts_with("::") {
            found.push(rest[start..end].to_string());
        }
        rest = &rest[end..];
    }
    found
}

fn csharp_imports(dir: &Path) -> Result<BTreeMap<String, BTreeSet<String>>> {
    let mut files = Vec::new();
    csharp_sources(dir, &mut files)?;
    let mut imports: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for file in files {
        let text = std::fs::read_to_string(&file)
            .with_context(|| format!("failed to read {}", file.display()))?;
        let constants = string_constants(&text);
        let lines: Vec<&str> = text.lines().map(str::trim).collect();
        for (index, line) in lines.iter().enumerate() {
            let Some(rest) = line
                .strip_prefix("[DllImport(")
                .or_else(|| line.strip_prefix("[LibraryImport("))
            else {
                continue;
            };
            let lib_end = rest
                .find([',', ')'])
                .with_context(|| format!("unterminated import attribute in {}", file.display()))?;
            let lib_expr = rest[..lib_end].trim();
            let lib = constants
                .get(lib_expr)
                .cloned()
                .unwrap_or_else(|| lib_expr.trim_matches('"').to_string());
            let symbol = entry_point(&rest[lib_end..])
                .or_else(|| declared_name(&lines[index + 1..]))
                .with_context(|| {
                    format!(
                        "cannot resolve the imported symbol at {}:{}",
                        file.display(),
                        index + 1
                    )
                })?;
            imports.entry(lib).or_default().insert(symbol.to_string());
        }
    }
    Ok(imports)
}

fn csharp_sources(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    if !dir.is_dir() {
        bail!("{} does not exist", dir.display());
    }
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            if path
                .file_name()
                .is_some_and(|name| name == "obj" || name == "bin")
            {
                continue;
            }
            csharp_sources(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "cs") {
            out.push(path);
        }
    }
    out.sort();
    Ok(())
}

fn string_constants(text: &str) -> BTreeMap<String, String> {
    const KEY: &str = "const string ";
    let mut constants = BTreeMap::new();
    for line in text.lines() {
        let Some(rest) = line.trim().split(KEY).nth(1) else {
            continue;
        };
        let Some((name, value)) = rest.split_once('=') else {
            continue;
        };
        let value = value.trim().trim_end_matches(';').trim();
        if let Some(literal) = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
            constants.insert(name.trim().to_string(), literal.to_string());
        }
    }
    constants
}

fn entry_point(attribute: &str) -> Option<&str> {
    let rest = attribute.split("EntryPoint").nth(1)?;
    let rest = rest.split_once('"')?.1;
    rest.split_once('"').map(|(name, _)| name)
}

fn declared_name<'a>(lines: &[&'a str]) -> Option<&'a str> {
    for line in lines.iter().take(3) {
        if line.starts_with('[') {
            continue;
        }
        let paren = line.find('(')?;
        let head = &line[..paren];
        let start = head.rfind(|c: char| !is_ident_char(c)).map_or(0, |i| i + 1);
        return (start < head.len()).then_some(&head[start..]);
    }
    None
}
