//! `modpack auth`: stores the platform secrets in the system keyring, through
//! the shared command ([`mc_core::auth`]).

use crate::context::Context;
use crate::error::Result;

pub use mc_core::auth::Args;

pub fn run(args: Args, ctx: &Context) -> Result<()> {
    mc_core::auth::run(args, &ctx.prompt, "modpack")
}
