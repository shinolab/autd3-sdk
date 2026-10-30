use std::path::Path;

use anyhow::{Result, bail};
use clap::Subcommand;

use crate::util::{capture, on_path, run};

#[derive(Subcommand)]
pub(crate) enum ActionsCmd {
    #[command(
        about = "Verify `.github/workflows/actions.lock` covers every workflow without writing anything"
    )]
    Lint,
    #[command(about = "Onboard new workflows and actions into `.github/workflows/actions.lock`")]
    Lock {
        #[arg(
            long,
            help = "Re-resolve every ref and advance moved ones to their current SHA"
        )]
        relock: bool,
    },
}

pub(crate) fn run_actions(root: &Path, cmd: &ActionsCmd) -> Result<()> {
    if !on_path("gh") {
        bail!("`gh` is required");
    }
    if !capture("gh", &["extension", "list"], root)?.contains("gh-actions-lock") {
        bail!("`gh actions-lock` is required: gh extension install github/gh-actions-lock");
    }
    match cmd {
        ActionsCmd::Lint => run("gh", ["actions-lock", "--verify", "--no-interactive"], root),
        ActionsCmd::Lock { relock } => {
            let mut args = vec!["actions-lock", "--no-interactive"];
            if *relock {
                args.push("--relock");
            }
            run("gh", args, root)
        }
    }
}
