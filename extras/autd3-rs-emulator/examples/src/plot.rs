use std::path::Path;
use std::process::Command;

pub fn visualize(csv: &Path) {
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
