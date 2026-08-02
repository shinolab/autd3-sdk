use std::time::Duration;

use eframe::egui;

use crate::panel::{AppliancePanel, FirmwarePanel, SimulatorPanel, TwinCatPanel};
use crate::update::Updater;

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum Tab {
    #[default]
    Simulator,
    Appliance,
    TwinCat,
    Firmware,
    About,
}

#[derive(Default)]
pub struct ConsoleApp {
    tab: Tab,
    simulator: SimulatorPanel,
    appliance: AppliancePanel,
    twincat: TwinCatPanel,
    firmware: FirmwarePanel,
    updater: Updater,
}

impl ConsoleApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut app = Self::default();
        if let Some(storage) = cc.storage {
            if let Some(config) = eframe::get_value(storage, "simulator") {
                app.simulator.config = config;
            }
            if let Some(config) = eframe::get_value(storage, "appliance") {
                app.appliance.config = config;
            }
            if let Some(config) = eframe::get_value(storage, "twincat") {
                app.twincat.config = config;
            }
            if let Some(config) = eframe::get_value(storage, "firmware") {
                app.firmware.config = config;
            }
            if let Some(config) = eframe::get_value(storage, "update") {
                app.updater.config = config;
            }
        }
        if !cfg!(target_os = "windows") {
            app.tab = Tab::Simulator;
        }
        if app.updater.config.auto_check {
            app.updater.check();
        }
        app
    }
}

impl eframe::App for ConsoleApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.simulator.pump();
        let poll_in = self.appliance.pump(self.tab == Tab::Appliance);
        self.twincat.pump();
        self.firmware.pump();
        self.updater.pump();
        if self.simulator.is_running()
            || self.appliance.is_running()
            || self.twincat.is_running()
            || self.firmware.is_running()
            || self.updater.is_busy()
        {
            ctx.request_repaint_after(Duration::from_millis(250));
        } else if let Some(poll_in) = poll_in {
            ctx.request_repaint_after(poll_in);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("tabs").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, Tab::Simulator, "Simulator");
                ui.selectable_value(&mut self.tab, Tab::Appliance, "Appliance");
                if cfg!(target_os = "windows") {
                    ui.selectable_value(&mut self.tab, Tab::TwinCat, "TwinCAT");
                }
                ui.selectable_value(&mut self.tab, Tab::Firmware, "Firmware");
                ui.selectable_value(&mut self.tab, Tab::About, "About");
            });
        });
        egui::CentralPanel::default().show(ui, |ui| {
            self.updater.banner(ui);
            match self.tab {
                Tab::Simulator => self.simulator.ui(ui),
                Tab::Appliance => self.appliance.ui(ui),
                Tab::TwinCat => self.twincat.ui(ui),
                Tab::Firmware => self.firmware.ui(ui),
                Tab::About => crate::about::ui(ui, &mut self.updater),
            }
        });
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, "simulator", &self.simulator.config);
        eframe::set_value(storage, "appliance", &self.appliance.config);
        eframe::set_value(storage, "twincat", &self.twincat.config);
        eframe::set_value(storage, "firmware", &self.firmware.config);
        eframe::set_value(storage, "update", &self.updater.config);
    }
}
