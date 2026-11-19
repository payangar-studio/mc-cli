//! `mod test`: the side matrix of the mod (`crate::sides`), on each of its
//! loaders: a dedicated server and a client with different mod sets, in two
//! processes.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use mc_core::fetch::Http;
use mc_core::server::{Machine, java};

use crate::context::Context;
use crate::error::{Error, Result};
use crate::model::mod_toml::{Loader, ModToml};
use crate::model::socle::SocleConfig;
use crate::sides::client::Launcher;
use crate::sides::matrix::{self, Case};
use crate::sides::{self, RUNS_DIR, Setup};
use crate::socle::SOCLE;

#[derive(Debug, clap::Args)]
pub struct Args {
    /// Loader to test (repeatable); every loader of mod.toml by default
    #[arg(long = "loader", value_name = "LOADER")]
    pub loaders: Vec<String>,
    /// Case to run, by name (repeatable); every case of the mod's side by default
    #[arg(long = "case", value_name = "CASE")]
    pub cases: Vec<String>,
    /// Game ticks a client stays on the server before the check that it is still there
    #[arg(long, default_value_t = 200)]
    pub ticks: u32,
}

pub fn run(args: Args, ctx: &Context) -> Result<()> {
    let root = ctx.require_root()?;
    let module = ModToml::load(root)?;
    let versions = SocleConfig::load(&SOCLE, root)?.init;
    let loaders = loaders(&module, &args.loaders)?;
    let cases = cases(&module, &args.cases)?;

    build(root, &loaders)?;

    let http = Http::new();
    let machine = Machine::from_env(&http, "mod")?;
    let java_major = match java::major_version(&machine.java).map_err(failed)? {
        Some(major) => major,
        None => {
            return Err(Error::usage(
                format!("no Java found (`{}`)", machine.java.display()),
                "set JAVA_HOME or put java on the PATH",
            ));
        }
    };
    let cache = machine
        .cache_dir
        .parent()
        .expect("the server cache is under the CLI's cache")
        .to_owned();
    let launcher =
        Launcher::install(&machine.java, &versions.minecraft, &cache, &http).map_err(failed)?;

    let mut failures = Vec::new();
    for loader in loaders {
        let mut mods = vec![sides::mod_jar(root, &module.id, loader).map_err(failed)?];
        mods.extend(
            sides::dependencies(&module, &versions, loader, &cache, &http).map_err(failed)?,
        );
        let setup = Setup {
            module: &module,
            versions: &versions,
            loader,
            mods,
            machine: &machine,
            java_major,
            launcher: &launcher,
            stay: Duration::from_millis(u64::from(args.ticks) * 50),
            runs: root.join(RUNS_DIR).join(loader.as_str()),
        };
        for case in &cases {
            match sides::run_case(&setup, case, &http) {
                Ok(()) => println!("PASS {loader} {}: {}", case.name, case.what),
                Err(reason) => {
                    println!(
                        "FAIL {loader} {}: {}\n  {}",
                        case.name,
                        case.what,
                        indent(&reason)
                    );
                    failures.push(format!("{loader} {}", case.name));
                }
            }
        }
    }
    if failures.is_empty() {
        return Ok(());
    }
    Err(Error::blocked(
        format!(
            "{} case(s) of the side matrix failed: {}",
            failures.len(),
            failures.join(", ")
        ),
        format!("read the logs of each case under {RUNS_DIR}/<loader>/<case>/"),
    ))
}

/// The loaders to test: those asked, each one of the mod's.
fn loaders(module: &ModToml, asked: &[String]) -> Result<Vec<Loader>> {
    if asked.is_empty() {
        return Ok(module.loaders.clone());
    }
    let names = module
        .loaders
        .iter()
        .map(|l| l.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    asked
        .iter()
        .map(|name| {
            Loader::parse(name)
                .filter(|l| module.loaders.contains(l))
                .ok_or_else(|| {
                    Error::usage(
                        format!("`{name}` is not a loader of this mod"),
                        format!("pass one of: {names}"),
                    )
                })
        })
        .collect()
}

/// The cases to run: those asked, each one of the mod's side.
fn cases(module: &ModToml, asked: &[String]) -> Result<Vec<Case>> {
    let all = matrix::cases(module.side);
    if asked.is_empty() {
        return Ok(all);
    }
    let names = all.iter().map(|c| c.name).collect::<Vec<_>>().join(", ");
    asked
        .iter()
        .map(|name| {
            all.iter().find(|c| c.name == name).copied().ok_or_else(|| {
                Error::usage(
                    format!("`{name}` is not a case of a `{}` mod", module.side),
                    format!("pass one of: {names}"),
                )
            })
        })
        .collect()
}

/// Builds the mod's jar of each loader with the repository's Gradle wrapper,
/// its output on stderr.
fn build(root: &Path, loaders: &[Loader]) -> Result<()> {
    let wrapper = if cfg!(windows) {
        "gradlew.bat"
    } else {
        "gradlew"
    };
    let tasks: Vec<String> = loaders.iter().map(|l| format!(":{l}:jar")).collect();
    tracing::info!(?tasks, "building the mod");
    let status = Command::new(root.join(wrapper))
        .arg("--console=plain")
        .args(&tasks)
        .current_dir(root)
        .stdout(Stdio::from(std::io::stderr()))
        .status()
        .map_err(|e| {
            Error::internal(
                format!("cannot run {wrapper}"),
                "restore the Gradle wrapper with `mod update --force`",
            )
            .with_source(e)
        })?;
    if status.success() {
        return Ok(());
    }
    Err(Error::blocked(
        format!("the build of the mod failed ({status})"),
        "fix the build: `./gradlew build` shows the error",
    ))
}

fn failed(reason: String) -> Error {
    Error::internal(
        reason,
        "the message says which step failed; rerun with -v to follow the steps",
    )
}

fn indent(text: &str) -> String {
    text.replace('\n', "\n  ")
}
