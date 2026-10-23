use std::time::Duration;

use eframe::egui;

use crate::panel::{FirmwarePanel, SimulatorPanel};

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum Tab {
    #[default]
    Simulator,
    Firmware,
    About,
}

#[derive(Default)]
pub(crate) struct ConsoleApp {
    tab: Tab,
    simulator: SimulatorPanel,
    firmware: FirmwarePanel,
}

impl ConsoleApp {
    pub(crate) fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut app = Self::default();
        if let Some(storage) = cc.storage {
            if let Some(config) = eframe::get_value(storage, "simulator") {
                app.simulator.config = config;
            }
            if let Some(config) = eframe::get_value(storage, "firmware") {
                app.firmware.config = config;
            }
        }
        app
    }
}

impl eframe::App for ConsoleApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.simulator.pump();
        self.firmware.pump();
        if self.simulator.is_running() || self.firmware.is_running() {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("tabs").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, Tab::Simulator, "Simulator");
                ui.selectable_value(&mut self.tab, Tab::Firmware, "Firmware");
                ui.selectable_value(&mut self.tab, Tab::About, "About");
            });
        });
        egui::CentralPanel::default().show(ui, |ui| match self.tab {
            Tab::Simulator => self.simulator.ui(ui),
            Tab::Firmware => self.firmware.ui(ui),
            Tab::About => crate::about::ui(ui),
        });
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, "simulator", &self.simulator.config);
        eframe::set_value(storage, "firmware", &self.firmware.config);
    }
}
