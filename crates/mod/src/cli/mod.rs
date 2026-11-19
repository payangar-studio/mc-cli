//! Command tree of the `mod` binary.
//!
//! Each command lives in `src/cli/<command>.rs`, which owns its `Args` and
//! its `run(args, ctx)` handler.

use clap::{Parser, Subcommand};

use crate::context::Context;
use crate::error::Result;

pub mod init;
pub mod port;
pub mod setup;
pub mod test;
pub mod update;

/// CLI of the Minecraft mod socle.
#[derive(Debug, Parser)]
#[command(name = "mod", version)]
pub struct Cli {
    #[command(flatten)]
    pub verbosity: Verbosity,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, clap::Args)]
pub struct Verbosity {
    /// More logs on stderr (-v info, -vv debug, -vvv trace)
    #[arg(short, long, action = clap::ArgAction::Count, global = true, help_heading = "Global options")]
    pub verbose: u8,
    /// Fewer logs on stderr (-q errors only, -qq none)
    #[arg(short, long, action = clap::ArgAction::Count, global = true, conflicts_with = "verbose", help_heading = "Global options")]
    pub quiet: u8,
}

impl Verbosity {
    /// `-v` count minus `-q` count.
    pub fn level(&self) -> i8 {
        self.verbose.min(10) as i8 - self.quiet.min(10) as i8
    }
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Create a mod repository: socle, Gradle build, entrypoints
    Init(init::Args),
    /// Bring the repository's socle to this CLI's version
    Update(update::Args),
    /// Configure the mod's GitHub repository: ruleset of the release tags, vault variables, CODEOWNERS
    Setup(setup::Args),
    /// Port the mod to another Minecraft version, on a new branch named after it
    Port(port::Args),
    /// Run the side matrix: a dedicated server and a client with different mod sets
    Test(test::Args),
}

pub fn run(command: Command, ctx: &Context) -> Result<()> {
    match command {
        Command::Init(args) => init::run(args, ctx),
        Command::Update(args) => update::run(args, ctx),
        Command::Setup(args) => setup::run(args, ctx),
        Command::Port(args) => port::run(args, ctx),
        Command::Test(args) => test::run(args, ctx),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn command_tree_is_valid() {
        Cli::command().debug_assert();
    }
}
