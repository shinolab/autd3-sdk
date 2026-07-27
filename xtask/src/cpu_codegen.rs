use std::fmt::Write as _;
use std::path::Path;

use anyhow::{Context, Result};

use crate::util::run;

const PARAMS_SVH_REL: &str = "firmware/fpga/rtl/sources_1/new/headers/params.svh";
const FW_OUT_REL: &str = "firmware/cpu/fw/src/params.rs";
const WIRE_OUT_REL: &str = "firmware/cpu/wire/src/params.rs";
const IP_DIR_REL: &str = "firmware/fpga/rtl/sources_1/ip";
const TR_COE_REL: &str = "firmware/fpga/coe/tr.coe";

struct IpConst {
    name: &'static str,
    ip: &'static str,
    elem_bits: u64,
}

const CLK_WIZ_IP: &str = "clk_wiz";
const CLK_OUT_FREQ_KEY: &str = "CLKOUT1_REQUESTED_OUT_FREQ";

const IP_CONSTS: &[IpConst] = &[
    IpConst {
        name: "MOD_BUFFER_SAMPLES",
        ip: "BRAM_MOD",
        elem_bits: 8,
    },
    IpConst {
        name: "PWE_TABLE_SIZE",
        ip: "BRAM_PWE_TABLE",
        elem_bits: 16,
    },
];

const FW_INTERNAL_PREFIXES: &[&str] = &["ADDR_", "BRAM_SELECT_", "BRAM_CNT_SELECT_", "CTL_FLAG_"];

fn is_fw_internal(name: &str) -> bool {
    FW_INTERNAL_PREFIXES.iter().any(|p| name.starts_with(p))
}

struct Const {
    name: String,
    value: String,
}

struct Enum {
    name: String,
    consts: Vec<Const>,
}

fn rust_type(name: &str) -> &'static str {
    if name == "NUM_TRANSDUCERS"
        || name == "NUM_BANKS"
        || name == "EMISSION_SLOT_WORDS"
        || name == "FOCUS_WORDS"
    {
        "usize"
    } else if name == "EMISSION_MAX_INDICES" || name == "ULTRASOUND_FREQ_HZ" {
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

fn group_decimal(s: &str) -> String {
    let (sign, digits) = s.strip_prefix('-').map_or(("", s), |d| ("-", d));
    if digits.len() <= 5 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return s.to_string();
    }
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i != 0 && (digits.len() - i) % 3 == 0 {
            out.push('_');
        }
        out.push(c);
    }
    format!("{sign}{out}")
}

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
    group_decimal(value)
}

fn to_upper(name: &str) -> String {
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

fn xci_value_str<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let key_tok = format!("\"{key}\"");
    let start = text.find(&key_tok)? + key_tok.len();
    let after = &text[start..];
    let vpos = after.find("\"value\"")? + "\"value\"".len();
    let after = &after[vpos..];
    let q1 = after.find('"')?;
    let q2 = after[q1 + 1..].find('"')? + q1 + 1;
    Some(&after[q1 + 1..q2])
}

fn xci_uint(text: &str, key: &str) -> Option<u64> {
    xci_value_str(text, key)?.parse::<u64>().ok()
}

fn mhz_to_hz(s: &str) -> Option<u64> {
    let (int, frac) = s.split_once('.').unwrap_or((s, ""));
    if int.is_empty() || !int.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let Ok(frac_digits) = u32::try_from(frac.len()) else {
        return None;
    };
    if frac_digits > 6 || !frac.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let scale = 10u64.pow(6 - frac_digits);
    let frac_hz = if frac.is_empty() {
        0
    } else {
        frac.parse::<u64>().ok()? * scale
    };
    Some(int.parse::<u64>().ok()? * 1_000_000 + frac_hz)
}

fn read_clk_consts(root: &Path) -> Result<Vec<Const>> {
    let xci = root
        .join(IP_DIR_REL)
        .join(CLK_WIZ_IP)
        .join(format!("{CLK_WIZ_IP}.xci"));
    let text = std::fs::read_to_string(&xci)
        .with_context(|| format!("failed to read {}", xci.display()))?;
    let mhz = xci_value_str(&text, CLK_OUT_FREQ_KEY)
        .with_context(|| format!("missing {CLK_OUT_FREQ_KEY} in {}", xci.display()))?;
    let hz = mhz_to_hz(mhz).with_context(|| {
        format!(
            "cannot parse {CLK_OUT_FREQ_KEY}={mhz:?} in {}",
            xci.display()
        )
    })?;
    Ok(vec![Const {
        name: "FPGA_CLK_FREQ_HZ".to_string(),
        value: hz.to_string(),
    }])
}

fn read_ip_consts(root: &Path) -> Result<Vec<Const>> {
    let mut out = Vec::with_capacity(IP_CONSTS.len());
    for ic in IP_CONSTS {
        let xci = root
            .join(IP_DIR_REL)
            .join(ic.ip)
            .join(format!("{}.xci", ic.ip));
        let text = std::fs::read_to_string(&xci)
            .with_context(|| format!("failed to read {}", xci.display()))?;
        let depth = xci_uint(&text, "Write_Depth_A")
            .with_context(|| format!("missing Write_Depth_A in {}", xci.display()))?;
        let width = xci_uint(&text, "Write_Width_A")
            .with_context(|| format!("missing Write_Width_A in {}", xci.display()))?;
        let value = depth * width / ic.elem_bits;
        out.push(Const {
            name: ic.name.to_string(),
            value: value.to_string(),
        });
    }
    Ok(out)
}

fn emit_ip(out: &mut String, c: &Const) {
    let _ = writeln!(
        out,
        "pub const {}: usize = {};",
        c.name,
        group_decimal(&c.value)
    );
}

fn read_tr_bounds(root: &Path) -> Result<Vec<Const>> {
    let path = root.join(TR_COE_REL);
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let vector = text
        .split_once("memory_initialization_vector")
        .map_or("", |(_, rest)| rest);
    let mut x_max: u32 = 0;
    let mut y_max: u32 = 0;
    for tok in vector.split(|c: char| !c.is_ascii_hexdigit()) {
        if tok.len() != 8 {
            continue;
        }
        if let Ok(word) = u32::from_str_radix(tok, 16) {
            x_max = x_max.max(word >> 16);
            y_max = y_max.max(word & 0xFFFF);
        }
    }
    Ok(vec![
        Const {
            name: "FOCUS_TR_X_MAX".to_string(),
            value: x_max.to_string(),
        },
        Const {
            name: "FOCUS_TR_Y_MAX".to_string(),
            value: y_max.to_string(),
        },
    ])
}

fn emit_tr(out: &mut String, c: &Const) {
    let _ = writeln!(
        out,
        "pub const {}: i32 = {};",
        c.name,
        group_decimal(&c.value)
    );
}

fn emit_clk(out: &mut String, c: &Const) {
    let _ = writeln!(
        out,
        "pub const {}: u32 = {};",
        c.name,
        group_decimal(&c.value)
    );
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

fn emit_bit_mask(out: &mut String, c: &Const) {
    let mask = c.name.replace("_BIT", "");
    let _ = writeln!(
        out,
        "pub const {mask}: {} = 1 << {};",
        rust_type(&mask),
        c.name
    );
}

fn generate(
    text: &str,
    ip_consts: &[Const],
    tr_consts: &[Const],
    clk_consts: &[Const],
) -> (String, String) {
    let (consts, enums) = parse(text);

    let header = |src: &str, kind: &str| {
        format!(
            "// AUTO-GENERATED by `cargo xtask cpu gen-param` from\n// {src}. DO NOT EDIT.\n// {kind}\n\n"
        )
    };
    let mut wire = header(
        &format!("{PARAMS_SVH_REL} and the IP .xci files"),
        "Client-facing wire constants shared with the firmware.",
    );
    let mut fw = header(PARAMS_SVH_REL, "Firmware-internal FPGA register map.");
    fw.push_str("pub use autd3_cpu_wire::params::*;\n");

    for c in &consts {
        emit(
            if is_fw_internal(&c.name) {
                &mut fw
            } else {
                &mut wire
            },
            c,
        );
    }
    for e in &enums {
        let Some(first) = e.consts.first() else {
            continue;
        };
        let out = if is_fw_internal(&first.name) {
            &mut fw
        } else {
            &mut wire
        };
        out.push('\n');
        let is_bit = e.name.ends_with("bit_t");
        for c in &e.consts {
            emit(out, c);
            if is_bit {
                emit_bit_mask(out, c);
            }
        }
    }

    if !ip_consts.is_empty() {
        wire.push('\n');
        for c in ip_consts {
            emit_ip(&mut wire, c);
        }
    }
    if !tr_consts.is_empty() {
        wire.push('\n');
        for c in tr_consts {
            emit_tr(&mut wire, c);
        }
    }
    if !clk_consts.is_empty() {
        wire.push('\n');
        for c in clk_consts {
            emit_clk(&mut wire, c);
        }
    }
    (wire, fw)
}

pub fn gen_param(root: &Path) -> Result<()> {
    let svh = root.join(PARAMS_SVH_REL);
    let text = std::fs::read_to_string(&svh)
        .with_context(|| format!("failed to read {}", svh.display()))?;

    let ip_consts = read_ip_consts(root)?;
    let tr_consts = read_tr_bounds(root)?;
    let clk_consts = read_clk_consts(root)?;
    let (wire_src, fw_src) = generate(&text, &ip_consts, &tr_consts, &clk_consts);
    for (rel, src) in [(WIRE_OUT_REL, wire_src), (FW_OUT_REL, fw_src)] {
        let out = root.join(rel);
        std::fs::write(&out, src).with_context(|| format!("failed to write {}", out.display()))?;
        run(
            "rustfmt",
            ["--edition", "2024", &out.to_string_lossy()],
            root,
        )
        .with_context(|| format!("rustfmt failed on {rel} (is it installed?)"))?;
    }
    Ok(())
}
