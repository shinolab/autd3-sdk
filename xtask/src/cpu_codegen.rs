use std::fmt::Write as _;
use std::path::Path;

use anyhow::{Context, Result};

use crate::util::run;

const PARAMS_SVH_REL: &str = "firmware/fpga/rtl/sources_1/new/headers/params.svh";
const OUT_REL: &str = "firmware/cpu/fw/src/params.rs";

struct Const {
    name: String,
    value: String,
}

struct Enum {
    name: String,
    consts: Vec<Const>,
}

fn rust_type(name: &str) -> &'static str {
    if name == "NUM_TRANSDUCERS" || name == "NUM_BANKS" {
        "usize"
    } else if name == "EMISSION_MAX_INDICES" {
        "u32"
    } else if name == "NUM_FOCI_MAX"
        || name.starts_with("VERSION_NUM_")
        || name.starts_with("BRAM_SELECT_")
        || name.starts_with("BRAM_CNT_SELECT_")
        || name.starts_with("TRANSITION_MODE_")
        || name.starts_with("EMISSION_TYPE_")
        || name.starts_with("GPIO_O_TYPE_")
        || name.starts_with("SILENCER_FLAG_")
    {
        "u8"
    } else {
        // ADDR_* and everything else default to u16.
        "u16"
    }
}

fn is_ident(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

fn is_decimal(v: &str) -> bool {
    !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit())
}

fn is_sized(v: &str) -> bool {
    let Some((size, rest)) = v.split_once('\'') else {
        return false;
    };
    if size.is_empty() || !size.bytes().all(|b| b.is_ascii_digit()) || rest.len() < 2 {
        return false;
    }
    matches!(rest.as_bytes()[0], b'b' | b'h' | b'd')
        && rest[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

fn is_float(v: &str) -> bool {
    let b = v.as_bytes();
    if b.len() < 3 {
        return false;
    }
    (1..b.len() - 1)
        .any(|k| b[..k].iter().all(u8::is_ascii_digit) && b[k + 1..].iter().all(u8::is_ascii_digit))
}

/// Convert a SystemVerilog sized literal to a Rust hex literal, honoring the base
/// prefix (`'b`/`'h`/`'d`). Plain decimals are passed through.
fn to_value(value: &str) -> String {
    if let Some((size, rest)) = value.split_once('\'') {
        if !size.is_empty() && size.bytes().all(|b| b.is_ascii_digit()) && rest.len() >= 2 {
            let radix = match rest.as_bytes()[0] {
                b'b' => 2,
                b'h' => 16,
                b'd' => 10,
                _ => return value.to_string(),
            };
            let digits = &rest[1..];
            if digits.bytes().all(|b| b.is_ascii_hexdigit())
                && let Ok(v) = u64::from_str_radix(digits, radix)
            {
                return format!("0x{v:X}");
            }
        }
        return value.to_string();
    }
    value.to_string()
}

/// `CamelCase` -> `UPPER_SNAKE_CASE`, matching the legacy `gen_param.py` heuristic.
fn to_upper(name: &str) -> String {
    // (.)([A-Z][a-z]+) -> \1_\2
    let chars: Vec<char> = name.chars().collect();
    let mut s1 = String::new();
    let mut i = 0;
    while i < chars.len() {
        if i + 2 < chars.len()
            && chars[i + 1].is_ascii_uppercase()
            && chars[i + 2].is_ascii_lowercase()
        {
            s1.push(chars[i]);
            s1.push('_');
            s1.push(chars[i + 1]);
            let mut j = i + 2;
            while j < chars.len() && chars[j].is_ascii_lowercase() {
                s1.push(chars[j]);
                j += 1;
            }
            i = j;
        } else {
            s1.push(chars[i]);
            i += 1;
        }
    }

    // ([a-z0-9])([A-Z]) -> \1_\2
    let chars: Vec<char> = s1.chars().collect();
    let mut s2 = String::new();
    let mut i = 0;
    while i < chars.len() {
        if i + 1 < chars.len()
            && (chars[i].is_ascii_lowercase() || chars[i].is_ascii_digit())
            && chars[i + 1].is_ascii_uppercase()
        {
            s2.push(chars[i]);
            s2.push('_');
            s2.push(chars[i + 1]);
            i += 2;
        } else {
            s2.push(chars[i]);
            i += 1;
        }
    }

    s2.to_ascii_uppercase()
}

fn parse_localparam(line: &str) -> Option<Const> {
    let rest = line.strip_prefix("localparam ")?;
    let (decl, valpart) = rest.split_once(" = ")?;
    let name = decl.split_whitespace().last()?;
    if !is_ident(name) {
        return None;
    }
    let value = valpart.split(';').next()?.trim();
    if !(is_decimal(value) || is_sized(value) || is_float(value)) {
        return None;
    }
    Some(Const {
        name: to_upper(name),
        value: value.to_string(),
    })
}

fn parse_member(s: &str) -> Option<Const> {
    let s = s.trim();
    let s = s.strip_suffix(',').unwrap_or(s);
    let (lhs, rhs) = s.split_once('=')?;
    let name = lhs.trim();
    let value = rhs.trim();
    if !is_ident(name) || !(is_decimal(value) || is_sized(value)) {
        return None;
    }
    Some(Const {
        name: name.to_string(),
        value: value.to_string(),
    })
}

fn enum_name(line: &str) -> Option<String> {
    let idx = line.rfind('}')?;
    let after = line[idx + 1..].trim();
    let name = after.strip_suffix(';').unwrap_or(after).trim();
    is_ident(name).then(|| name.to_string())
}

fn parse(text: &str) -> (Vec<Const>, Vec<Enum>) {
    let mut consts = Vec::new();
    let mut enums = Vec::new();
    let mut read_enum = false;
    let mut enum_consts: Vec<Const> = Vec::new();

    for raw in text.lines() {
        let line = raw.trim();
        if let Some(c) = parse_localparam(line) {
            consts.push(c);
        } else if line.starts_with("typedef enum") {
            read_enum = true;
            enum_consts = Vec::new();
            if let Some(open) = line.find('{')
                && let Some(close) = line[open + 1..].find('}')
                && let Some(c) = parse_member(&line[open + 1..open + 1 + close])
            {
                enum_consts.push(c);
            }
            if line.contains('}') {
                read_enum = false;
                if let Some(name) = enum_name(line) {
                    enums.push(Enum {
                        name,
                        consts: std::mem::take(&mut enum_consts),
                    });
                }
            }
        } else if read_enum {
            if line.starts_with('}') {
                read_enum = false;
                if let Some(name) = enum_name(line) {
                    enums.push(Enum {
                        name,
                        consts: std::mem::take(&mut enum_consts),
                    });
                }
            } else if let Some(c) = parse_member(line) {
                enum_consts.push(c);
            }
        }
    }

    (consts, enums)
}

fn emit(out: &mut String, c: &Const) {
    let _ = writeln!(
        out,
        "pub const {}: {} = {};",
        c.name,
        rust_type(&c.name),
        to_value(&c.value)
    );
}

fn generate(text: &str) -> String {
    let (consts, enums) = parse(text);

    let mut out = format!(
        "// AUTO-GENERATED by `cargo xtask cpu gen-param` from\n// {PARAMS_SVH_REL}. DO NOT EDIT.\n\n"
    );
    for c in &consts {
        emit(&mut out, c);
    }
    for e in &enums {
        out.push('\n');
        let is_bit = e.name.ends_with("bit_t");
        for c in &e.consts {
            emit(&mut out, c);
            if is_bit {
                let mask = c.name.replace("_BIT", "");
                let _ = writeln!(
                    out,
                    "pub const {mask}: {} = 1 << {};",
                    rust_type(&mask),
                    c.name
                );
            }
        }
    }
    out
}

/// Regenerate `firmware/cpu/fw/src/params.rs` from the FPGA `params.svh`, the single
/// source of truth for the hardware constants.
pub fn gen_param(root: &Path) -> Result<()> {
    let svh = root.join(PARAMS_SVH_REL);
    let text = std::fs::read_to_string(&svh)
        .with_context(|| format!("failed to read {}", svh.display()))?;

    let out = root.join(OUT_REL);
    std::fs::write(&out, generate(&text))
        .with_context(|| format!("failed to write {}", out.display()))?;

    run(
        "rustfmt",
        ["--edition", "2024", &out.to_string_lossy()],
        root,
    )
    .context("rustfmt failed on the generated params.rs (is it installed?)")?;
    Ok(())
}
