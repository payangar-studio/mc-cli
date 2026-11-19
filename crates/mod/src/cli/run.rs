//! `mod run`: the mod in the game, alone or with extra mods and resource
//! packs from Modrinth: once (`--with`, `--pack`, in `runs/.one-shot/`,
//! emptied first) or through a test profile (`--profile`, in `runs/<name>/`).
//! `--server` adds a dedicated dev server the client joins.

use mc_core::error::Context as _;

use crate::context::Context;
use crate::error::{Error, Result};
use crate::model::mod_toml::{Loader, ModToml};
use crate::model::socle::SocleConfig;
use crate::model::test_profile::{TestProfile, Wanted};
use crate::run::{self, ONE_SHOT, Plan, RUNS};
use crate::socle::SOCLE;

#[derive(Debug, clap::Args)]
pub struct Args {
    /// Test profile to run (test-profiles/<name>.toml), in runs/<name>/
    #[arg(long, value_name = "NAME", conflicts_with_all = ["with", "pack"])]
    pub profile: Option<String>,
    /// Modrinth mod for this run only, as <slug> or <slug>@<version>; repeatable
    #[arg(long = "with", value_name = "MOD")]
    pub with: Vec<String>,
    /// Modrinth resource pack for this run only, as <slug> or <slug>@<version>; repeatable
    #[arg(long = "pack", value_name = "RESOURCE_PACK")]
    pub pack: Vec<String>,
    /// Loader to run: the profile's, else the mod's first one
    #[arg(long)]
    pub loader: Option<String>,
    /// Also start a dedicated dev server (offline, RCON on) and join it
    #[arg(long)]
    pub server: bool,
}

pub fn run(args: Args, ctx: &Context) -> Result<()> {
    let root = ctx.require_root()?;
    let module = ModToml::load(root)?;
    let socle = SocleConfig::load(&SOCLE, root)?;
    let runs = root.join(RUNS);

    let (dir, profile) = match &args.profile {
        Some(name) => (runs.join(name), TestProfile::load(root, name)?),
        None => {
            let dir = runs.join(ONE_SHOT);
            if dir.exists() {
                std::fs::remove_dir_all(&dir).context(format!("emptying {}", dir.display()))?;
            }
            let profile = TestProfile {
                mods: args.with.clone(),
                packs: args.pack.clone(),
                ..TestProfile::default()
            };
            (dir, profile)
        }
    };
    let loader = loader(&args, &profile, &module)?;
    let wanted = |list: &[String]| {
        list.iter()
            .map(|w| Wanted::parse(w))
            .collect::<Result<Vec<_>>>()
    };
    let plan = Plan {
        dir,
        loader,
        mods: wanted(&profile.mods)?,
        packs: wanted(&profile.packs)?,
        options: profile.options_lines()?,
        server: args.server,
    };
    run::run(
        root,
        &plan,
        &run::pack(&socle.init, loader),
        &run::builtin(),
    )
}

/// The loader to run: `--loader`, the profile's, else the mod's first one;
/// always one of the mod's.
fn loader(args: &Args, profile: &TestProfile, module: &ModToml) -> Result<Loader> {
    let names = || {
        module
            .loaders
            .iter()
            .map(|l| l.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let loader = match &args.loader {
        Some(name) => Loader::parse(name).ok_or_else(|| {
            Error::usage(
                format!("unknown loader `{name}`"),
                format!("use one of the mod's loaders: {}", names()),
            )
        })?,
        None => profile.loader.unwrap_or(module.loaders[0]),
    };
    if !module.loaders.contains(&loader) {
        return Err(Error::usage(
            format!("the mod is not built for {loader}"),
            format!("use one of the mod's loaders: {}", names()),
        ));
    }
    Ok(loader)
}
