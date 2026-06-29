use std::time::Duration;

use eframe::egui;

use crate::panel::{SimulatorPanel, TwinCatPanel};

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum Tab {
    #[default]
    Simulator,
    TwinCat,
    About,
}

#[derive(Default)]
pub struct ConsoleApp {
    tab: Tab,
    simulator: SimulatorPanel,
    twincat: TwinCatPanel,
}

impl ConsoleApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut app = Self::default();
        if let Some(storage) = cc.storage {
            if let Some(config) = eframe::get_value(storage, "simulator") {
                app.simulator.config = config;
            }
            if let Some(config) = eframe::get_value(storage, "twincat") {
                app.twincat.config = config;
            }
        }
        if !cfg!(target_os = "windows") {
            app.tab = Tab::Simulator;
        }
        app
    }
}

impl eframe::App for ConsoleApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.simulator.pump();
        self.twincat.pump();
        if self.simulator.is_running() || self.twincat.is_running() {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("tabs").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, Tab::Simulator, "Simulator");
                if cfg!(target_os = "windows") {
                    ui.selectable_value(&mut self.tab, Tab::TwinCat, "TwinCAT");
                }
                ui.selectable_value(&mut self.tab, Tab::About, "About");
            });
        });
        egui::CentralPanel::default().show(ui, |ui| match self.tab {
            Tab::Simulator => self.simulator.ui(ui),
            Tab::TwinCat => self.twincat.ui(ui),
            Tab::About => crate::about::ui(ui),
        });
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, "simulator", &self.simulator.config);
        eframe::set_value(storage, "twincat", &self.twincat.config);
    }
}
