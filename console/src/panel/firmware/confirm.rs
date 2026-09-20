use eframe::egui;

use crate::process::FINISHED;

use super::Target;

#[derive(Default, Clone, PartialEq, Eq, Debug)]
pub struct DeviceVersions {
    pub cpu: Option<String>,
    pub fpga: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Answer {
    Pending,
    Update,
    Cancel,
}

pub struct Confirm {
    pub version: String,
    pub target: Target,
    pub devices: Vec<DeviceVersions>,
}

enum Part {
    Cpu,
    Fpga,
}

fn parse_line(line: &str) -> Option<(usize, Part, String)> {
    let (device, rest) = line.strip_prefix("device ")?.split_once(": ")?;
    let device = device.parse::<usize>().ok()?;
    let (part, version) = match rest.strip_prefix("CPU firmware now = ") {
        Some(version) => (Part::Cpu, version),
        None => (Part::Fpga, rest.strip_prefix("FPGA firmware now = ")?),
    };
    Some((device, part, version.trim().to_string()))
}

pub fn parse(logs: &[String]) -> Vec<DeviceVersions> {
    let mut devices: Vec<DeviceVersions> = Vec::new();
    for (device, part, version) in logs.iter().filter_map(|line| parse_line(line)) {
        if devices.len() <= device {
            devices.resize(device + 1, DeviceVersions::default());
        }
        match part {
            Part::Cpu => devices[device].cpu = Some(version),
            Part::Fpga => devices[device].fpga = Some(version),
        }
    }
    devices
}

pub fn outcome(logs: &[String]) -> Result<Vec<DeviceVersions>, String> {
    if logs.last().map(String::as_str) != Some(FINISHED) {
        return Err("reading the current firmware version failed; see the log below".to_string());
    }
    let devices = parse(logs);
    if devices.is_empty() {
        return Err("no device reported its firmware version; see the log below".to_string());
    }
    Ok(devices)
}

impl Confirm {
    pub fn ui(&self, ui: &egui::Ui) -> Answer {
        let mut answer = Answer::Pending;
        let modal = egui::Modal::new(egui::Id::new("firmware-ota-confirm")).show(ui.ctx(), |ui| {
            ui.set_width(460.0);
            ui.heading(format!("Update the firmware to v{}?", self.version));
            ui.add_space(4.0);
            ui.label(format!("{} device(s) on the bus", self.devices.len()));
            ui.add_space(4.0);
            egui::Grid::new("firmware-ota-confirm-grid")
                .num_columns(4)
                .spacing([12.0, 4.0])
                .striped(true)
                .show(ui, |ui| {
                    for (device, versions) in self.devices.iter().enumerate() {
                        for (part, current, updated) in [
                            ("CPU", versions.cpu.as_deref(), self.target.cpu()),
                            ("FPGA", versions.fpga.as_deref(), self.target.fpga()),
                        ] {
                            let Some(current) = current else {
                                continue;
                            };
                            ui.label(format!("device {device}"));
                            ui.label(part);
                            ui.monospace(current);
                            if updated {
                                ui.monospace(format!("-> {}", self.version));
                            } else {
                                ui.weak("(unchanged)");
                            }
                            ui.end_row();
                        }
                    }
                });
            ui.add_space(4.0);
            ui.colored_label(
                egui::Color32::LIGHT_YELLOW,
                "Transducer output stops during the update; do not power off the devices \
                 while it runs.",
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Update").clicked() {
                    answer = Answer::Update;
                }
                if ui.button("Cancel").clicked() {
                    answer = Answer::Cancel;
                }
            });
        });
        if answer == Answer::Pending && modal.should_close() {
            answer = Answer::Cancel;
        }
        answer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn logs(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|l| (*l).to_string()).collect()
    }

    #[test]
    fn both_versions_of_every_device_are_read_from_the_verify_output() {
        let devices = parse(&logs(&[
            "2 device(s) on the bus",
            "device 0: CPU firmware now = 0.9.0",
            "device 1: CPU firmware now = 0.9.1",
            "2 device(s) on the bus",
            "device 0: FPGA firmware now = 0.9.0 (Update image)",
            "device 1: FPGA firmware now = 0.9.0 (Golden image)",
            "[process finished]",
        ]));
        assert_eq!(
            devices,
            [
                DeviceVersions {
                    cpu: Some("0.9.0".to_string()),
                    fpga: Some("0.9.0 (Update image)".to_string()),
                },
                DeviceVersions {
                    cpu: Some("0.9.1".to_string()),
                    fpga: Some("0.9.0 (Golden image)".to_string()),
                },
            ]
        );
    }

    #[test]
    fn a_single_target_leaves_the_other_side_unknown() {
        let devices = parse(&logs(&[
            "device 0: FPGA firmware now = 0.9.0 (Update image)",
        ]));
        assert_eq!(
            devices,
            [DeviceVersions {
                cpu: None,
                fpga: Some("0.9.0 (Update image)".to_string()),
            }]
        );
    }

    #[test]
    fn only_a_finished_verify_run_that_reported_a_device_opens_the_dialog() {
        assert_eq!(
            outcome(&logs(&[
                "device 0: CPU firmware now = 0.9.0",
                "[process finished]"
            ]))
            .unwrap()[0]
                .cpu
                .as_deref(),
            Some("0.9.0")
        );
        assert!(
            outcome(&logs(&[
                "device 0: CPU firmware now = 0.9.0",
                "[process exited with code 1]",
            ]))
            .is_err()
        );
        assert!(outcome(&logs(&["[process finished]"])).is_err());
    }

    #[test]
    fn the_dialog_waits_until_a_button_is_pressed() {
        let confirm = Confirm {
            version: "0.10.0".to_string(),
            target: Target::Cpu,
            devices: parse(&logs(&[
                "device 0: CPU firmware now = 0.9.0",
                "device 0: FPGA firmware now = 0.9.0 (Update image)",
            ])),
        };
        egui::__run_test_ui(|ui| assert_eq!(confirm.ui(ui), Answer::Pending));
    }

    #[test]
    fn unrelated_lines_are_ignored_and_gaps_are_filled() {
        assert!(parse(&logs(&["image: 1024 bytes, crc32 0x00000000"])).is_empty());
        let devices = parse(&logs(&[
            "device 1: CPU firmware now = 0.9.0",
            "device x: CPU firmware now = 0.9.0",
            "device 0: CPU firmware before = 0.9.0",
        ]));
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0], DeviceVersions::default());
        assert_eq!(devices[1].cpu.as_deref(), Some("0.9.0"));
    }
}
