mod app;
mod emulator;
mod geometry;
#[cfg(test)]
mod harness;
mod render;
mod scene;
mod settings;

use anyhow::{Context, Result};
use autd3_cpu_wire::udp::PORT;
use autd3_rs::geometry::{Autd3, Geometry};
use clap::Parser;
use eframe::egui;
use tracing_subscriber::EnvFilter;

use crate::app::SimulatorApp;
use crate::scene::Scene;

#[derive(Parser)]
struct Args {
    #[arg(long, default_value_t = PORT, value_parser = clap::value_parser!(u16).range(1..))]
    port: u16,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
    let args = Args::parse();

    let geometry = Geometry::new(vec![Autd3::default()]);
    let scene = Scene::new(args.port, geometry.num_devices())
        .context("binding the emulated devices; is another simulator running?")?;
    tracing::info!(
        "emulated devices on [::1]:{}; set the geometry in the Geometry tab",
        scene.port()
    );

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 860.0])
            .with_min_inner_size([640.0, 480.0])
            .with_title("AUTD3 Simulator"),
        ..Default::default()
    };
    eframe::run_native(
        "autd3-simulator",
        options,
        Box::new(move |cc| Ok(Box::new(SimulatorApp::new(cc, scene, &geometry)))),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))
}
