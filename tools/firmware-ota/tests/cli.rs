use std::process::Command;

fn ota(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_autd3-firmware-ota"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn a_release_older_than_the_udp_firmware_is_refused_before_anything_is_fetched_or_sent() {
    for target in ["both", "cpu", "fpga"] {
        let output = ota(&[
            "--version",
            "0.9.0",
            "--target",
            target,
            "--devices",
            "1",
            "--simulator",
        ]);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{stderr}");
        assert!(stderr.contains("J-Link"), "{stderr}");
        assert!(!stderr.contains("Downloading"), "{stderr}");
        assert!(!stderr.contains("Using cached"), "{stderr}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    }
}

#[test]
fn the_help_does_not_offer_a_release_older_than_the_udp_firmware() {
    let output = ota(&["--help"]);
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(help.contains("e.g. 0.10.0"), "{help}");
    assert!(!help.contains("e.g. 0.9"), "{help}");
    assert!(help.contains("not checked"), "{help}");
}
