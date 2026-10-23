use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::process::Command;

use autd3_rs_emulator::{RawColumn, RawFrame};

pub(crate) fn save_csv(path: &Path, frames: &[&RawFrame]) -> std::io::Result<()> {
    let mut out = BufWriter::new(File::create(path)?);
    let columns: Vec<&(String, RawColumn)> =
        frames.iter().flat_map(|frame| &frame.columns).collect();
    let header: Vec<&str> = columns.iter().map(|(name, _)| name.as_str()).collect();
    writeln!(out, "{}", header.join(","))?;
    let rows = frames.iter().map(|frame| frame.rows).min().unwrap_or(0);
    for row in 0..rows {
        let cells: Vec<String> = columns
            .iter()
            .map(|(_, column)| match column {
                RawColumn::U8(v) => v[row].to_string(),
                RawColumn::U16(v) => v[row].to_string(),
                RawColumn::F32(v) => v[row].to_string(),
            })
            .collect();
        writeln!(out, "{}", cells.join(","))?;
    }
    out.flush()
}

pub(crate) fn visualize(csv: &Path) {
    if std::env::args().any(|a| a == "--no-plot") {
        println!(
            "--no-plot specified; skipping matplotlib (CSV: {})",
            csv.display()
        );
        return;
    }
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("plot_field.py");
    match Command::new("python3").arg(&script).arg(csv).status() {
        Ok(status) if status.success() => {}
        Ok(status) => eprintln!("python3 exited with {status} (CSV: {})", csv.display()),
        Err(e) => eprintln!(
            "could not launch python3 ({e}); CSV saved at {}",
            csv.display()
        ),
    }
}
