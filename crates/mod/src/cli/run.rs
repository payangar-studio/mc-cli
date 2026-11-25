//! `mod run`: the mod in the game, alone or with extra mods and resource
//! packs from Modrinth or GitHub releases: once (`--with`, `--pack`, in `runs/.one-shot/`,
//! emptied first) or through a test profile (`--profile`, in `runs/<name>/`).
//! `--server` adds a dedicated dev server the client joins. `--prepare-only`
//! fills the run directory without starting the game: the mod's CI does it
//! for a CI profile before its gametests.

use mc_core::error::Context as _;

use crate::context::Context;
use crate::error::{Error, Result};
use crate::model::mod_toml::{Loader, ModToml};
use crate::model::socle::SocleConfig;
use crate::model::test_profile::{Item, TestProfile, Wanted};
use crate::run::{self, ONE_SHOT, Plan, RUNS};
use crate::socle::SOCLE;

#[derive(Debug, clap::Args)]
pub struct Args {
    /// Test profile to run (test-profiles/<name>.toml), in runs/<name>/
    #[arg(long, value_name = "NAME", conflicts_with_all = ["with", "pack"])]
    pub profile: Option<String>,
    /// Mod for this run only: from Modrinth as <slug>[@<version>], from GitHub as github:<owner>/<repo>[@<tag>]; repeatable
    #[arg(long = "with", value_name = "MOD")]
    pub with: Vec<String>,
    /// Resource pack for this run only, written as for --with; repeatable
    #[arg(long = "pack", value_name = "RESOURCE_PACK")]
    pub pack: Vec<String>,
    /// Loader to run: the profile's, else the mod's first one
    #[arg(long)]
    pub loader: Option<String>,
    /// Also start a dedicated dev server (offline, RCON on) and join it
    #[arg(long)]
    pub server: bool,
    /// Download the mods and resource packs and write the options, without starting the game
    #[arg(long)]
    pub prepare_only: bool,
}

pub fn run(args: Args, ctx: &Context) -> Result<()> {
    let root = ctx.require_root()?;
    let module = ModToml::load(root)?;
    let socle = SocleConfig::load(&SOCLE, root)?;
    let runs = root.join(RUNS);

    let one_shot = args.profile.is_none();
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
    // Local files are a profile's: `--with` and `--pack` take platform projects.
    let items = |list: &[String]| {
        list.iter()
            .map(|w| {
                if one_shot {
                    Wanted::parse(w).map(Item::Platform)
                } else {
                    Item::parse(w)
                }
            })
            .collect::<Result<Vec<_>>>()
    };
    let plan = Plan {
        dir,
        loader,
        mods: items(&profile.mods)?,
        packs: items(&profile.packs)?,
        local: TestProfile::local_dir(root),
        options: profile.options_lines()?,
        server: args.server,
    };
    let pack = run::pack(&socle.init, loader);
    if args.prepare_only {
        run::prepare(&plan, &pack, &run::builtin())?;
        println!("Run directory: {}", plan.dir.display());
        return Ok(());
    }
    run::run(root, &plan, &pack, &run::builtin())
}

/// The loader to run: `--loader`, the profile's, else Fabric for a CI profile
/// and the mod's first loader otherwise; always one of the mod's.
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
        None => match (profile.loader, profile.ci) {
            (Some(loader), _) => loader,
            (None, true) => Loader::Fabric,
            (None, false) => *module.loaders.first().ok_or_else(|| {
                Error::usage(
                    "the mod has no loader",
                    "list its loaders in mod.toml, such as `loaders = [\"fabric\"]`",
                )
            })?,
        },
    };
    if !module.loaders.contains(&loader) {
        return Err(Error::usage(
            format!("the mod is not built for {loader}"),
            format!("use one of the mod's loaders: {}", names()),
        ));
    }
    Ok(loader)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::mod_toml::Side;

    fn module(loaders: Vec<Loader>) -> ModToml {
        ModToml {
            id: "swift_blades".into(),
            name: "Swift Blades".into(),
            package: "com.payangar.swiftblades".into(),
            side: Side::BothRequired,
            loaders,
            license: "MIT".into(),
            dependencies: Vec::new(),
        }
    }

    fn args(loader: Option<&str>) -> Args {
        Args {
            profile: Some("p".into()),
            with: Vec::new(),
            pack: Vec::new(),
            loader: loader.map(str::to_owned),
            server: false,
            prepare_only: false,
        }
    }

    #[test]
    fn a_ci_profile_runs_on_fabric_whatever_the_first_loader() {
        let module = module(vec![Loader::Neoforge, Loader::Fabric]);
        let ci = TestProfile {
            ci: true,
            ..TestProfile::default()
        };
        assert_eq!(loader(&args(None), &ci, &module).unwrap(), Loader::Fabric);
        assert_eq!(
            loader(&args(None), &TestProfile::default(), &module).unwrap(),
            Loader::Neoforge
        );
    }

    #[test]
    fn a_ci_profile_of_a_mod_without_fabric_is_refused() {
        let ci = TestProfile {
            ci: true,
            ..TestProfile::default()
        };
        let err = loader(&args(None), &ci, &module(vec![Loader::Neoforge])).unwrap_err();
        assert!(err.to_string().contains("not built for fabric"), "{err}");
    }

    #[test]
    fn a_mod_without_loaders_is_a_usage_error() {
        let err = loader(&args(None), &TestProfile::default(), &module(Vec::new())).unwrap_err();
        assert!(err.to_string().contains("no loader"), "{err}");
        assert!(err.hint().contains("mod.toml"), "{}", err.hint());
    }
}
