//! `mod init`: creates a mod repository.

use std::path::{Path, PathBuf};
use std::process::Command;

use mc_core::machine_config::{IdentityField, MachineIdentity};
use mc_core::project::{Project, ProjectType};

use crate::context::{Context, SOCLE_DIR};
use crate::error::{Context as _, Error, Result};
use crate::model::mod_toml::{
    self, Dependency, Loader, ModToml, Side, check_id, check_package, default_package,
};
use crate::versions;

/// Loader sets offered in a terminal, as `--loaders` takes them.
const LOADER_SETS: &[&str] = &["fabric,neoforge", "fabric", "neoforge"];

#[derive(Debug, clap::Args)]
pub struct Args {
    /// Directory to create the repository in
    #[arg(default_value = ".")]
    pub dir: PathBuf,
    /// Mod id: a lowercase letter, then lowercase letters, digits or `_`
    #[arg(long)]
    pub id: Option<String>,
    /// Display name of the mod
    #[arg(long)]
    pub name: Option<String>,
    /// Java package; defaults to `<package>.<id without _>`, `package` being under `[identity]` in the machine configuration
    #[arg(long)]
    pub package: Option<String>,
    /// Where the mod runs: client, server, both-required, both-optional
    #[arg(long)]
    pub side: Option<String>,
    /// Loaders, comma-separated: fabric, neoforge
    #[arg(long)]
    pub loaders: Option<String>,
    /// License, as an SPDX id such as `MIT`
    #[arg(long)]
    pub license: Option<String>,
    /// Minecraft version, which also names the git branch
    #[arg(long)]
    pub mc_version: Option<String>,
}

pub fn run(args: Args, ctx: &Context) -> Result<()> {
    let dir = args.dir;
    if let Some(other) = Project::at(&dir).filter(|p| p.kind != ProjectType::Mod) {
        return Err(other.wrong_type(ProjectType::Mod.binary()));
    }
    if dir.join(SOCLE_DIR).exists() {
        return Err(Error::usage(
            format!("{} already holds a mod socle", dir.display()),
            format!(
                "run `mod update` in {} to bring its socle up to date, or pick another directory",
                dir.display()
            ),
        ));
    }

    let p = &ctx.prompt;
    let id = p.text(args.id, "--id", "Mod id")?;
    check_id(&id)?;
    let name = p.text(args.name, "--name", "Mod name")?;
    let package = match args.package {
        Some(package) => package,
        None => {
            let base =
                MachineIdentity::new().resolve(p, None, IdentityField::Package, "--package")?;
            default_package(&base, &id)
        }
    };
    check_package(&package)?;
    let sides: Vec<String> = Side::ALL.iter().map(|s| s.to_string()).collect();
    let side = p.select(args.side, "--side", "Side", &sides)?;
    let side = Side::ALL
        .into_iter()
        .find(|s| s.as_str() == side)
        .ok_or_else(|| {
            Error::usage(
                format!("unknown side `{side}`"),
                format!("pass --side with one of: {}", sides.join(", ")),
            )
        })?;
    let loaders: Vec<String> = LOADER_SETS.iter().map(|s| s.to_string()).collect();
    let loaders = parse_loaders(&p.select(args.loaders, "--loaders", "Loaders", &loaders)?)?;
    let license = p.text(args.license, "--license", "License (SPDX id)")?;
    let supported: Vec<String> = versions::SUPPORTED
        .iter()
        .map(|v| v.minecraft.to_owned())
        .collect();
    let minecraft = p.select(
        args.mc_version,
        "--mc-version",
        "Minecraft version",
        &supported,
    )?;
    let pinned = versions::of(&minecraft).ok_or_else(|| {
        Error::usage(
            format!("Minecraft {minecraft} is not supported by this CLI"),
            format!("pass --mc-version with one of: {}", supported.join(", ")),
        )
    })?;

    let module = ModToml {
        dependencies: default_dependencies(&loaders),
        id,
        name,
        package,
        side,
        loaders,
        license,
    };
    std::fs::create_dir_all(&dir).context(format!("creating {}", dir.display()))?;
    git_init(&dir, &minecraft)?;
    crate::socle::init(&dir, &module, pinned.versions())?;

    // No final period: the directory is often `.`, and `in ..` reads as the parent.
    println!("Created mod `{}` in {}", module.name, dir.display());
    println!(
        "Review {}, then build with `./gradlew build`.",
        mod_toml::FILE_NAME
    );
    Ok(())
}

/// `fabric,neoforge` as loaders, each once, in a stable order.
fn parse_loaders(value: &str) -> Result<Vec<Loader>> {
    let mut loaders = Vec::new();
    for name in value.split(',').map(str::trim) {
        let loader = Loader::parse(name).ok_or_else(|| {
            Error::usage(
                format!("unknown loader `{name}`"),
                "pass --loaders with fabric, neoforge, or both separated by a comma",
            )
        })?;
        if !loaders.contains(&loader) {
            loaders.push(loader);
        }
    }
    loaders.sort();
    Ok(loaders)
}

/// Fabric API, which nearly every Fabric mod needs: declared, it is on the
/// Fabric classpath and in the published dependencies. The mod removes it from
/// `mod.toml` if it does without.
fn default_dependencies(loaders: &[Loader]) -> Vec<Dependency> {
    if !loaders.contains(&Loader::Fabric) {
        return Vec::new();
    }
    vec![Dependency {
        id: "fabric-api".into(),
        kind: "required".into(),
        loaders: Some(vec![Loader::Fabric]),
        modrinth: Some("fabric-api".into()),
        curseforge: Some("fabric-api".into()),
    }]
}

/// Makes `dir` a git repository on a branch named after the Minecraft version,
/// unless it already is one. Never commits.
fn git_init(dir: &Path, minecraft: &str) -> Result<()> {
    if dir.join(".git").exists() {
        return Ok(());
    }
    let hint = format!(
        "install git, or run `git init --initial-branch={minecraft}` in {} yourself, then rerun `mod init`",
        dir.display()
    );
    let output = Command::new("git")
        .arg("init")
        .arg("--quiet")
        .arg(format!("--initial-branch={minecraft}"))
        .current_dir(dir)
        .output()
        .map_err(|e| Error::internal("could not run `git init`", hint.clone()).with_source(e))?;
    if !output.status.success() {
        return Err(Error::internal(
            format!(
                "`git init` failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
            hint,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loaders_are_parsed_once_in_order() {
        assert_eq!(
            parse_loaders("neoforge, fabric,neoforge").unwrap(),
            [Loader::Fabric, Loader::Neoforge]
        );
        assert!(parse_loaders("forge").is_err());
        assert!(parse_loaders("").is_err());
    }

    #[test]
    fn fabric_api_is_declared_for_fabric_only() {
        assert!(default_dependencies(&[Loader::Neoforge]).is_empty());
        let deps = default_dependencies(&[Loader::Fabric, Loader::Neoforge]);
        assert_eq!(deps[0].id, "fabric-api");
        assert_eq!(deps[0].loaders, Some(vec![Loader::Fabric]));
    }
}
