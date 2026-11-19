//! `modpack update`: brings the repository's socle to this CLI's version,
//! through the shared socle mechanics ([`mc_core::socle::Socle::update`]).

use crate::context::Context;
use crate::error::Result;
use crate::model::socle::InitValues;
use crate::socle::SOCLE;

#[derive(Debug, clap::Args)]
pub struct Args {
    /// Overwrite socle files edited by hand
    #[arg(long)]
    pub force: bool,
}

pub fn run(args: Args, ctx: &Context) -> Result<()> {
    let root = ctx.require_root_unchecked()?;
    SOCLE.update::<InitValues>(root, args.force)
}
