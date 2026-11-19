//! The project page, as versioned in `page/`: `page.toml` (name, summary,
//! links, one table per platform), `description.md`, `icon.png` and
//! `gallery/` with its optional `gallery.toml`. Each platform pushes it its
//! own way; [`push`] is the `page push` command of every binary.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use sha1::{Digest, Sha1};

use crate::error::{Context, Error, Result};
use crate::model::entry::Env;
use crate::model::side::Side;
use crate::platform::{Capability, Operation, Platform, Registry, unsupported};

/// Directory of the page, relative to the repository root.
pub const DIR: &str = "page";
pub const PAGE_TOML: &str = "page.toml";
pub const DESCRIPTION: &str = "description.md";
pub const ICON: &str = "icon.png";
pub const GALLERY_DIR: &str = "gallery";
pub const GALLERY_TOML: &str = "gallery.toml";
/// Extensions Modrinth accepts for images, with their media type.
pub const IMAGE_TYPES: &[(&str, &str)] = &[
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
    ("bmp", "image/bmp"),
];
/// The page as versioned in `page/`, with what the platforms need of it.
#[derive(Debug)]
pub struct Page {
    /// The repository root.
    pub root: PathBuf,
    /// The binary of the repository's type (`modpack`), as messages name
    /// its commands.
    pub bin: String,
    pub name: String,
    pub summary: String,
    pub body: String,
    pub links: Links,
    pub modrinth: ModrinthSection,
    pub icon: Option<Image>,
    pub gallery: Vec<GalleryImage>,
}

/// What a project is, as the platforms file it: set when the project is
/// created, never changed by a later push.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Listing {
    pub project_type: ProjectType,
    /// What each side needs to run the project.
    pub env: Env,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectType {
    Mod,
    Modpack,
}

impl ProjectType {
    /// How the platforms (Modrinth's `project_type`) and messages name it.
    pub fn name(self) -> &'static str {
        match self {
            ProjectType::Mod => "mod",
            ProjectType::Modpack => "modpack",
        }
    }
}

impl Listing {
    /// A mod, whose sides follow the `side` of its `mod.toml`.
    pub fn of_mod(side: Side) -> Self {
        Self {
            project_type: ProjectType::Mod,
            env: side.env(),
        }
    }
}

/// `page/page.toml`. Other platforms' tables are left to them.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct PageToml {
    name: String,
    summary: String,
    links: Links,
    modrinth: ModrinthSection,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Links {
    pub source: String,
    pub issues: String,
    pub wiki: String,
    pub discord: String,
}

/// The `[modrinth]` table of `page.toml`.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ModrinthSection {
    pub project_id: String,
    pub slug: String,
    pub categories: Vec<String>,
    pub additional_categories: Vec<String>,
    pub license: String,
}

/// `page/gallery/gallery.toml`.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct GalleryToml {
    image: Vec<GalleryEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GalleryEntry {
    file: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    featured: bool,
}

/// An image of the page, read whole.
#[derive(Debug)]
pub struct Image {
    /// File name.
    pub file: String,
    /// Extension, lowercase.
    pub ext: String,
    pub media_type: &'static str,
    pub bytes: Vec<u8>,
    /// Hex sha1 of `bytes`.
    pub sha1: String,
}

#[derive(Debug)]
pub struct GalleryImage {
    pub image: Image,
    pub title: Option<String>,
    pub description: Option<String>,
    pub featured: bool,
}

impl Page {
    /// Reads `page/` under the repository `root`. An empty `name` falls back
    /// to `default_name`, read only then. `bin` is the binary whose `init`
    /// lays the page down (`modpack`), for the error when there is none.
    pub fn load(
        root: &Path,
        bin: &str,
        default_name: impl FnOnce() -> Result<String>,
    ) -> Result<Self> {
        let dir = root.join(DIR);
        let toml_path = dir.join(PAGE_TOML);
        let text = fs::read_to_string(&toml_path).map_err(|err| {
            Error::usage(
                format!("cannot read {DIR}/{PAGE_TOML}: {err}"),
                format!(
                    "create {DIR}/ with {PAGE_TOML} and {DESCRIPTION}; `{bin} init` lays them down in a new repository"
                ),
            )
        })?;
        let toml: PageToml = toml::from_str(&text).map_err(|err| {
            Error::usage(
                format!("{DIR}/{PAGE_TOML} is invalid: {}", err.message()),
                format!("fix {DIR}/{PAGE_TOML}"),
            )
        })?;
        let mut name = toml.name.trim().to_owned();
        if name.is_empty() {
            name = default_name()?;
        }
        let summary = toml.summary.trim().to_owned();
        if summary.is_empty() {
            return Err(Error::usage(
                format!("{DIR}/{PAGE_TOML} has no summary"),
                format!("set `summary` in {DIR}/{PAGE_TOML}: one or two sentences"),
            ));
        }
        let body = fs::read_to_string(dir.join(DESCRIPTION)).map_err(|err| {
            Error::usage(
                format!("cannot read {DIR}/{DESCRIPTION}: {err}"),
                format!("write the long description of the pack in {DIR}/{DESCRIPTION}"),
            )
        })?;
        let icon_path = dir.join(ICON);
        let icon = icon_path
            .is_file()
            .then(|| Image::read(&icon_path))
            .transpose()?;
        Ok(Self {
            root: root.to_path_buf(),
            bin: bin.to_owned(),
            name,
            summary,
            body,
            links: toml.links,
            modrinth: toml.modrinth,
            icon,
            gallery: read_gallery(&dir.join(GALLERY_DIR))?,
        })
    }
}

/// `<bin> page push`: pushes the page to `only`, or to every platform of
/// `registry`. A platform with an API pushes it and its report goes to
/// `out`; for one that goes through a skill, `out` names the skill. The page
/// is read by `load`, once, when the first platform with an API needs it.
pub fn push<P: ?Sized + Platform>(
    registry: &Registry<P>,
    only: Option<&str>,
    load: impl FnOnce() -> Result<Page>,
    listing: &Listing,
    out: &mut impl Write,
) -> Result<()> {
    let platforms = match only {
        Some(name) => vec![registry.get(name)?],
        None => registry.all().collect(),
    };
    let mut load = Some(load);
    let mut page = None;
    for platform in platforms {
        let name = platform.name();
        match platform.capability(Operation::PushPage) {
            Capability::Api => {
                if let Some(load) = load.take() {
                    page = Some(load()?);
                }
                let page = page
                    .as_ref()
                    .expect("the page is read before the first push");
                writeln!(out, "{}", platform.push_page(page, listing)?)?;
            }
            Capability::Skill(skill) => writeln!(
                out,
                "{name}: no API for the page; run the `{skill}` skill to push it"
            )?,
            Capability::Unsupported if only.is_some() => {
                return Err(unsupported(name, Operation::PushPage));
            }
            Capability::Unsupported => writeln!(out, "{name}: no project page, skipped")?,
        }
    }
    Ok(())
}

/// Refuses `<bin> page push` when `ci` (the `CI` variable) says it runs in
/// CI: the project page is managed in a session, never in CI.
pub fn refuse_in_ci(ci: Option<&str>, bin: &str) -> Result<()> {
    if ci.is_some_and(|v| !v.is_empty() && v != "false" && v != "0") {
        return Err(Error::usage(
            format!("`{bin} page push` manages the project page, which is never done in CI"),
            "run it in a session, from your own machine",
        ));
    }
    Ok(())
}

impl Image {
    pub fn read(path: &Path) -> Result<Self> {
        let file = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let (ext, media_type) = image_type(path).ok_or_else(|| {
            Error::usage(
                format!("{file} is not an image Modrinth accepts"),
                format!(
                    "use one of these extensions: {}",
                    IMAGE_TYPES
                        .iter()
                        .map(|(e, _)| *e)
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )
        })?;
        let bytes = fs::read(path).context(format!("reading {}", path.display()))?;
        let sha1 = hex(&Sha1::digest(&bytes));
        Ok(Self {
            file,
            ext,
            media_type,
            bytes,
            sha1,
        })
    }
}

/// Extension and media type of an image file the platforms accept.
pub fn image_type(path: &Path) -> Option<(String, &'static str)> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    let media_type = IMAGE_TYPES.iter().find(|(e, _)| *e == ext)?.1;
    Some((ext, media_type))
}

/// The gallery in display order: the images `gallery.toml` lists, in its
/// order, then the other images of the directory by file name.
fn read_gallery(dir: &Path) -> Result<Vec<GalleryImage>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let toml_path = dir.join(GALLERY_TOML);
    let listed: GalleryToml = if toml_path.is_file() {
        let text = fs::read_to_string(&toml_path)
            .context(format!("reading {DIR}/{GALLERY_DIR}/{GALLERY_TOML}"))?;
        toml::from_str(&text).map_err(|err| {
            Error::usage(
                format!(
                    "{DIR}/{GALLERY_DIR}/{GALLERY_TOML} is invalid: {}",
                    err.message()
                ),
                format!("fix {DIR}/{GALLERY_DIR}/{GALLERY_TOML}"),
            )
        })?
    } else {
        GalleryToml::default()
    };

    let mut gallery = Vec::new();
    for entry in listed.image {
        let path = dir.join(&entry.file);
        if !path.is_file() {
            return Err(Error::usage(
                format!(
                    "{DIR}/{GALLERY_DIR}/{GALLERY_TOML} lists `{}`, which is not in {DIR}/{GALLERY_DIR}/",
                    entry.file
                ),
                "add the image, or remove its entry",
            ));
        }
        gallery.push(GalleryImage {
            image: Image::read(&path)?,
            title: non_empty(entry.title),
            description: non_empty(entry.description),
            featured: entry.featured,
        });
    }
    let mut others: Vec<PathBuf> = fs::read_dir(dir)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<_>>()?;
    others.retain(|p| p.is_file() && image_type(p).is_some());
    others.sort();
    for path in others {
        let image = Image::read(&path)?;
        if gallery.iter().all(|g| g.image.file != image.file) {
            gallery.push(GalleryImage {
                image,
                title: None,
                description: None,
                featured: false,
            });
        }
    }
    if gallery.iter().filter(|g| g.featured).count() > 1 {
        return Err(Error::usage(
            format!("{DIR}/{GALLERY_DIR}/{GALLERY_TOML} features more than one image"),
            "keep `featured = true` on one image only",
        ));
    }
    Ok(gallery)
}

fn non_empty(text: String) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::error::ErrorKind;
    use crate::platform::fake::FakePlatform;

    /// A platform whose page capability is given, recording the pushes.
    struct Recorder {
        name: &'static str,
        capability: Capability,
        pushed: Arc<Mutex<Vec<String>>>,
    }

    impl Platform for Recorder {
        fn name(&self) -> &str {
            self.name
        }
        fn capability(&self, _: Operation) -> Capability {
            self.capability.clone()
        }
        fn push_page(&self, page: &Page, listing: &Listing) -> Result<String> {
            self.pushed.lock().unwrap().push(format!(
                "{} {} {}",
                self.name,
                page.name,
                listing.project_type.name()
            ));
            Ok(format!("{}: pushed", self.name))
        }
    }

    fn registry(pushed: &Arc<Mutex<Vec<String>>>) -> Registry {
        Registry::new(vec![
            Box::new(Recorder {
                name: "api",
                capability: Capability::Api,
                pushed: pushed.clone(),
            }),
            Box::new(FakePlatform::new("fake")),
            Box::new(Recorder {
                name: "none",
                capability: Capability::Unsupported,
                pushed: pushed.clone(),
            }),
        ])
    }

    fn page() -> Page {
        Page {
            root: PathBuf::from("/repo"),
            bin: "test".into(),
            name: "Swift Blades".into(),
            summary: "Faster swords.".into(),
            body: String::new(),
            links: Links::default(),
            modrinth: ModrinthSection::default(),
            icon: None,
            gallery: Vec::new(),
        }
    }

    /// Pushes with a page loader that counts its calls.
    fn run_push(registry: &Registry, only: Option<&str>) -> (Result<()>, String, usize) {
        let mut out = Vec::new();
        let mut loads = 0;
        let listing = Listing::of_mod(Side::BothRequired);
        let result = push(
            registry,
            only,
            || {
                loads += 1;
                Ok(page())
            },
            &listing,
            &mut out,
        );
        (result, String::from_utf8(out).unwrap(), loads)
    }

    #[test]
    fn every_platform_gets_the_page_its_own_way() {
        let pushed = Arc::default();
        let (result, out, loads) = run_push(&registry(&pushed), None);
        result.unwrap();
        assert_eq!(*pushed.lock().unwrap(), ["api Swift Blades mod"]);
        assert_eq!(loads, 1);
        assert_eq!(
            out,
            "api: pushed\n\
             fake: no API for the page; run the `modpack-fake-page` skill to push it\n\
             none: no project page, skipped\n"
        );
    }

    #[test]
    fn platform_option_pushes_to_that_platform_only() {
        let pushed = Arc::default();
        let registry = registry(&pushed);

        let (result, out, loads) = run_push(&registry, Some("fake"));
        result.unwrap();
        assert!(pushed.lock().unwrap().is_empty());
        assert!(out.contains("`modpack-fake-page`"));
        // A skill reads the page itself: the CLI does not need it.
        assert_eq!(loads, 0);

        let (result, out, _) = run_push(&registry, Some("api"));
        result.unwrap();
        assert_eq!(*pushed.lock().unwrap(), ["api Swift Blades mod"]);
        assert_eq!(out, "api: pushed\n");
    }

    #[test]
    fn named_platform_must_exist_and_support_the_page() {
        let pushed = Arc::default();
        let registry = registry(&pushed);
        let (result, _, _) = run_push(&registry, Some("nope"));
        assert_eq!(result.unwrap_err().kind(), ErrorKind::Usage);
        let (result, _, _) = run_push(&registry, Some("none"));
        let err = result.unwrap_err();
        assert!(err.to_string().contains("does not support `push_page`"));
    }

    #[test]
    fn refused_in_ci() {
        let err = refuse_in_ci(Some("true"), "mod").unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Usage);
        assert!(err.to_string().contains("`mod page push`"));
        assert!(refuse_in_ci(Some("false"), "mod").is_ok());
        assert!(refuse_in_ci(Some(""), "mod").is_ok());
        assert!(refuse_in_ci(None, "mod").is_ok());
    }
}
