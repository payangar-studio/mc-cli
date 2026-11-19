//! `modpack page push`: pushes the versioned page (`page/`) to the platforms
//! that manage a project by their API, and names the skill to run for the
//! others ([`mc_core::page::push`]). Project management happens in a session,
//! never in CI.

use std::path::Path;

use mc_core::model::entry::{Env, Support};
use mc_core::page::{Listing, Page, ProjectType};

use crate::context::Context;
use crate::error::Result;
use crate::model::socle::SocleConfig;
use crate::socle::SOCLE;

/// How a modpack is listed on the platforms: played on the client, which a
/// server may or may not run.
pub const LISTING: Listing = Listing {
    project_type: ProjectType::Modpack,
    env: Env {
        client: Support::Required,
        server: Support::Optional,
    },
};

#[derive(Debug, clap::Args)]
pub struct Args {
    /// Push to this platform only
    #[arg(long)]
    pub platform: Option<String>,
}

pub fn run(args: Args, ctx: &Context) -> Result<()> {
    mc_core::page::refuse_in_ci(std::env::var("CI").ok().as_deref(), SOCLE.bin)?;
    let root = ctx.require_root()?;
    mc_core::page::push(
        &crate::platform::builtin(),
        args.platform.as_deref(),
        || load(root),
        &LISTING,
        &mut std::io::stdout(),
    )
}

/// Reads `page/` under the repository `root`. An empty `name` falls back to
/// the pack name given to `modpack init`.
pub fn load(root: &Path) -> Result<Page> {
    Page::load(root, SOCLE.bin, || {
        Ok(SocleConfig::load(&SOCLE, root)?.init.name)
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::model::socle::InitValues;

    #[test]
    fn a_modpack_is_listed_as_before_the_shared_page_push() {
        assert_eq!(LISTING.project_type, ProjectType::Modpack);
        assert_eq!(LISTING.env.client, Support::Required);
        assert_eq!(LISTING.env.server, Support::Optional);
    }

    #[test]
    fn without_a_page_says_where_it_is_expected() {
        let dir = tempfile::tempdir().unwrap();
        let err = load(dir.path()).unwrap_err();
        assert!(err.to_string().contains("cannot read page/page.toml"));
    }

    /// The skeleton `modpack init` lays down: the pack name comes from init,
    /// and the summary is asked for before anything is sent.
    #[test]
    fn page_template_needs_a_summary_and_takes_the_pack_name() {
        let dir = tempfile::tempdir().unwrap();
        crate::socle::init(
            dir.path(),
            InitValues {
                name: "Skeleton Pack".into(),
                minecraft: "1.21.1".into(),
                loader: "neoforge".into(),
                loader_version: "21.1.77".into(),
                author: "Tester".into(),
            },
        )
        .unwrap();
        let err = load(dir.path()).unwrap_err();
        assert!(err.to_string().contains("has no summary"));

        let path = dir.path().join("page/page.toml");
        let text = fs::read_to_string(&path).unwrap().replacen(
            "summary = \"\"",
            "summary = \"Filled.\"",
            1,
        );
        fs::write(&path, text).unwrap();
        let page = load(dir.path()).unwrap();
        assert_eq!(page.name, "Skeleton Pack");
        assert_eq!(page.bin, "modpack");
        assert!(page.icon.is_none());
        assert!(page.gallery.is_empty());
        assert!(page.modrinth.project_id.is_empty());
    }
}
