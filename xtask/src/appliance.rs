mod image;
mod server;

use std::path::Path;

use anyhow::Result;
use clap::Subcommand;

use image::ImageCmd;
use server::ServerCmd;

#[derive(Subcommand)]
pub enum ApplianceCmd {
    /// The EtherCAT master appliance server (`appliance/server/`).
    Server {
        #[command(subcommand)]
        cmd: ServerCmd,
    },
    /// The appliance SD image (`appliance/image/<board>/`, Linux + root).
    Image {
        #[command(subcommand)]
        cmd: ImageCmd,
    },
}

pub fn run_appliance(root: &Path, cmd: &ApplianceCmd) -> Result<()> {
    match cmd {
        ApplianceCmd::Server { cmd } => server::run_server(root, cmd),
        ApplianceCmd::Image { cmd } => image::run_image(root, cmd),
    }
}
