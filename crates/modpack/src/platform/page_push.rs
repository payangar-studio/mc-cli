//! `push_page` of Modrinth: pushes the page versioned in `page/`
//! ([`mc_core::page`]) to the project, by the API.
//!
//! Without a project id in `page/page.toml`, the project is created as a
//! draft modpack and its id is written back to `page.toml`. With one, the
//! project's texts, links, categories and license are edited. In both cases
//! the icon and the gallery are then brought in line with `page/`: the
//! repository is the source of truth, so an image removed from `page/gallery/`
//! is removed from Modrinth.
//!
//! Modrinth names every uploaded image after the sha1 of its bytes
//! (`<sha1>.<ext>`, or `<sha1>_<width>.webp` once resized), which is how a
//! local image is matched to a remote one without recording anything else.

use std::fs;
use std::path::Path;

use serde::Deserialize;
use serde_json::{Value, json};
use ureq::http::{Response, StatusCode};
use ureq::typestate::WithBody;
use ureq::{Body, RequestBuilder};

use mc_core::page::{DIR, GalleryImage, Image, PAGE_TOML, Page};
use mc_core::platform::modrinth::client::Client;
use mc_core::platform::modrinth::{NAME, encode_segment};

use crate::error::{Context, Error, Result};
use crate::model::socle::SocleConfig;
use crate::secrets::Secret;
use crate::socle::SOCLE;

/// Sides declared at creation, required by the API. A modpack is played on
/// the client; a server may or may not run it.
const CLIENT_SIDE: &str = "required";
const SERVER_SIDE: &str = "optional";

/// Reads `page/` under the repository `root`. An empty `name` falls back to
/// the pack name given to `modpack init`.
pub fn load(root: &Path) -> Result<Page> {
    Page::load(root, SOCLE.bin, || {
        Ok(SocleConfig::load(&SOCLE, root)?.init.name)
    })
}

/// The project as Modrinth returns it, reduced to what the push compares.
#[derive(Debug, Deserialize)]
struct Remote {
    id: String,
    slug: Option<String>,
    icon_url: Option<String>,
    #[serde(default)]
    gallery: Vec<RemoteImage>,
}

#[derive(Debug, Deserialize)]
struct RemoteImage {
    url: String,
    featured: bool,
    title: Option<String>,
    description: Option<String>,
    #[serde(default)]
    ordering: i64,
}

/// The sha1 Modrinth named an uploaded image after, from its URL.
fn url_sha1(url: &str) -> &str {
    let name = url.rsplit('/').next().unwrap_or(url);
    name.split(['.', '_']).next().unwrap_or(name)
}

/// What a push changed, for the line printed at the end.
#[derive(Debug, Default, PartialEq, Eq)]
struct Changes {
    icon: bool,
    added: usize,
    updated: usize,
    removed: usize,
}

/// Pushes `page` to Modrinth with `token`, and returns a line saying what
/// was done.
pub fn push(client: &Client, page: &Page, token: &Secret) -> Result<String> {
    let api = Api { client, token };
    let section = &page.modrinth;
    let mut renamed = None;
    let (remote, created) = if section.project_id.trim().is_empty() {
        let remote = api.create(page)?;
        record_project_id(&page.root, &remote.id)?;
        (remote, true)
    } else {
        let id = section.project_id.trim();
        let mut remote = api.project(id)?;
        // An empty slug is only derived from the name at creation: it never
        // renames an existing project.
        let wanted = section.slug.trim();
        if !wanted.is_empty() && remote.slug.as_deref() != Some(wanted) {
            renamed = Some(remote.slug.replace(wanted.to_owned()));
        }
        api.edit(&remote.id, page, renamed.is_some().then_some(wanted))?;
        (remote, false)
    };
    let mut changes = Changes {
        icon: sync_icon(&api, &remote, page.icon.as_ref())?,
        ..Changes::default()
    };
    sync_gallery(&api, &remote, &page.gallery, &mut changes)?;

    let slug = remote.slug.as_deref().unwrap_or(&remote.id);
    let mut line = if created {
        format!(
            "{NAME}: created draft project {slug} ({}), id recorded in {DIR}/{PAGE_TOML}; submit it for review on Modrinth once the page is ready",
            remote.id
        )
    } else {
        format!("{NAME}: updated project {slug} ({})", remote.id)
    };
    if let Some(old) = renamed {
        let old = old.as_deref().unwrap_or("none");
        line.push_str(&format!("; slug changed from {old} to {slug}"));
    }
    if changes.icon {
        line.push_str("; icon uploaded");
    }
    line.push_str(&format!(
        "; gallery: {} added, {} updated, {} removed",
        changes.added, changes.updated, changes.removed
    ));
    Ok(line)
}

/// Writes the project id into the `[modrinth]` table of `page.toml`,
/// keeping the rest of the file as it is.
fn record_project_id(root: &Path, id: &str) -> Result<()> {
    let path = root.join(DIR).join(PAGE_TOML);
    let text = fs::read_to_string(&path).context(format!("reading {DIR}/{PAGE_TOML}"))?;
    let mut doc: toml_edit::DocumentMut =
        text.parse().context(format!("parsing {DIR}/{PAGE_TOML}"))?;
    if !doc.contains_table(NAME) {
        doc[NAME] = toml_edit::table();
    }
    doc[NAME]["project_id"] = toml_edit::value(id);
    fs::write(&path, doc.to_string()).context(format!("writing {DIR}/{PAGE_TOML}"))
}

fn sync_icon(api: &Api, remote: &Remote, icon: Option<&Image>) -> Result<bool> {
    let Some(icon) = icon else {
        return Ok(false);
    };
    let current = remote.icon_url.as_deref().map(url_sha1);
    if current.is_some_and(|sha1| sha1.eq_ignore_ascii_case(&icon.sha1)) {
        return Ok(false);
    }
    api.write(
        Method::Patch,
        &format!("/project/{}/icon", encode_segment(&remote.id)),
        &[("ext", icon.ext.clone())],
        Some(icon),
    )?;
    Ok(true)
}

fn sync_gallery(
    api: &Api,
    remote: &Remote,
    gallery: &[GalleryImage],
    changes: &mut Changes,
) -> Result<()> {
    let path = format!("/project/{}/gallery", encode_segment(&remote.id));
    let local = |r: &RemoteImage| {
        gallery
            .iter()
            .any(|g| url_sha1(&r.url).eq_ignore_ascii_case(&g.image.sha1))
    };
    for stale in remote.gallery.iter().filter(|r| !local(r)) {
        api.write(Method::Delete, &path, &[("url", stale.url.clone())], None)?;
        changes.removed += 1;
    }
    for (ordering, wanted) in gallery.iter().enumerate() {
        let ordering = ordering as i64;
        let mut query = vec![
            ("featured", wanted.featured.to_string()),
            ("ordering", ordering.to_string()),
        ];
        query.extend(wanted.title.clone().map(|t| ("title", t)));
        query.extend(wanted.description.clone().map(|d| ("description", d)));
        let existing = remote
            .gallery
            .iter()
            .find(|r| url_sha1(&r.url).eq_ignore_ascii_case(&wanted.image.sha1));
        match existing {
            None => {
                query.push(("ext", wanted.image.ext.clone()));
                api.write(Method::Post, &path, &query, Some(&wanted.image))?;
                changes.added += 1;
            }
            Some(r) => {
                // An empty local title or description leaves the remote one:
                // the API cannot clear them.
                let same = r.featured == wanted.featured
                    && r.ordering == ordering
                    && wanted
                        .title
                        .as_ref()
                        .is_none_or(|t| r.title.as_ref() == Some(t))
                    && wanted
                        .description
                        .as_ref()
                        .is_none_or(|d| r.description.as_ref() == Some(d));
                if !same {
                    query.push(("url", r.url.clone()));
                    api.write(Method::Patch, &path, &query, None)?;
                    changes.updated += 1;
                }
            }
        }
    }
    Ok(())
}

/// Authenticated calls of the page push.
struct Api<'a> {
    client: &'a Client,
    token: &'a Secret,
}

impl Api<'_> {
    /// The project, which must exist (a draft is only visible with the token).
    fn project(&self, id: &str) -> Result<Remote> {
        let path = format!("/project/{}", encode_segment(id));
        let response = self.client.call(&format!("GET {path}"), |agent, api| {
            agent
                .get(format!("{api}{path}"))
                .header("Authorization", self.token.expose())
                .call()
        })?;
        if response.status() == StatusCode::NOT_FOUND {
            return Err(Error::usage(
                format!("Modrinth has no project `{id}`, or the token cannot see it"),
                format!(
                    "check `{NAME}.project_id` in {DIR}/{PAGE_TOML}; empty it to create a new project"
                ),
            ));
        }
        read(response, &path)
    }

    /// Creates the project as a draft modpack.
    fn create(&self, page: &Page) -> Result<Remote> {
        let section = &page.modrinth;
        if section.license.trim().is_empty() {
            return Err(Error::usage(
                format!(
                    "{DIR}/{PAGE_TOML} has no `{NAME}.license`, required to create the project"
                ),
                format!(
                    "set `{NAME}.license` to an SPDX id, such as `MIT` or `LicenseRef-All-Rights-Reserved`"
                ),
            ));
        }
        let slug = match section.slug.trim() {
            "" => slugify(&page.name),
            slug => slug.to_owned(),
        };
        let mut data = fields(page);
        data["slug"] = json!(slug);
        data["project_type"] = json!("modpack");
        data["client_side"] = json!(CLIENT_SIDE);
        data["server_side"] = json!(SERVER_SIDE);
        data["initial_versions"] = json!([]);
        data["is_draft"] = json!(true);
        let (content_type, body) = multipart_data(&data.to_string());
        let response = self.client.call("POST /project", |agent, api| {
            agent
                .post(format!("{api}/project"))
                .header("Authorization", self.token.expose())
                .header("Content-Type", &content_type)
                .send(&body[..])
        })?;
        read(response, "/project")
    }

    /// Edits the texts, links, categories and license of the project, and
    /// its slug when `slug` is given.
    fn edit(&self, id: &str, page: &Page, slug: Option<&str>) -> Result<()> {
        let path = format!("/project/{}", encode_segment(id));
        let mut data = fields(page);
        if let Some(slug) = slug {
            data["slug"] = json!(slug);
        }
        let response = self.client.call(&format!("PATCH {path}"), |agent, api| {
            agent
                .patch(format!("{api}{path}"))
                .header("Authorization", self.token.expose())
                .send_json(&data)
        })?;
        check(response, &path).map(drop)
    }

    /// A write without JSON body: `POST` or `PATCH` with the image as raw
    /// body or with nothing, `DELETE`; the parameters go in the query.
    fn write(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, String)],
        image: Option<&Image>,
    ) -> Result<()> {
        let pairs = query.iter().map(|(k, v)| (*k, v.as_str()));
        let response = self
            .client
            .call(&format!("{method} {path}"), |agent, api| {
                let url = format!("{api}{path}");
                let with_body = |request: RequestBuilder<WithBody>| {
                    let request = request
                        .header("Authorization", self.token.expose())
                        .query_pairs(pairs.clone());
                    match image {
                        Some(image) => request
                            .header("Content-Type", image.media_type)
                            .send(&image.bytes[..]),
                        None => request.send_empty(),
                    }
                };
                match method {
                    Method::Post => with_body(agent.post(&url)),
                    Method::Patch => with_body(agent.patch(&url)),
                    Method::Delete => agent
                        .delete(&url)
                        .header("Authorization", self.token.expose())
                        .query_pairs(pairs.clone())
                        .call(),
                }
            })?;
        check(response, path).map(drop)
    }
}

#[derive(Debug, Clone, Copy)]
enum Method {
    Post,
    Patch,
    Delete,
}

impl std::fmt::Display for Method {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Method::Post => "POST",
            Method::Patch => "PATCH",
            Method::Delete => "DELETE",
        })
    }
}

/// The fields create and edit share. An empty link is sent as `null`, which
/// clears it.
fn fields(page: &Page) -> Value {
    let link = |url: &str| match url.trim() {
        "" => Value::Null,
        url => json!(url),
    };
    let section = &page.modrinth;
    let mut data = json!({
        "title": page.name,
        "description": page.summary,
        "body": page.body,
        "categories": section.categories,
        "additional_categories": section.additional_categories,
        "source_url": link(&page.links.source),
        "issues_url": link(&page.links.issues),
        "wiki_url": link(&page.links.wiki),
        "discord_url": link(&page.links.discord),
    });
    if !section.license.trim().is_empty() {
        data["license_id"] = json!(section.license.trim());
    }
    data
}

/// A slug from the pack name: lowercase ASCII letters and digits, other
/// characters turned into single dashes.
fn slugify(name: &str) -> String {
    let mut slug = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.trim_matches('-').to_owned()
}

/// A `multipart/form-data` body with the one `data` part project creation
/// needs, and its content type.
fn multipart_data(json: &str) -> (String, Vec<u8>) {
    let mut boundary = String::from("modpack-cli-form-boundary");
    while json.contains(&boundary) {
        boundary.push('x');
    }
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"data\"\r\nContent-Type: application/json\r\n\r\n{json}\r\n--{boundary}--\r\n"
    );
    (
        format!("multipart/form-data; boundary={boundary}"),
        body.into_bytes(),
    )
}

/// The response, or an error saying what Modrinth refused and why.
fn check(mut response: Response<Body>, path: &str) -> Result<Response<Body>> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    // Modrinth explains a refusal in `{"error": ..., "description": ...}`.
    let reason = response
        .body_mut()
        .read_json::<Value>()
        .ok()
        .and_then(|v| v.get("description")?.as_str().map(str::to_owned))
        .map(|d| format!(": {d}"))
        .unwrap_or_default();
    let what = format!("Modrinth answered {status} to {path}{reason}");
    Err(match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Error::usage(
            what,
            "check the Modrinth token and its scopes (PROJECT_CREATE, PROJECT_WRITE), then store it with `modpack auth modrinth-token`",
        ),
        StatusCode::BAD_REQUEST if reason.to_ascii_lowercase().contains("slug") => Error::usage(
            what,
            format!(
                "set `{NAME}.slug` in {DIR}/{PAGE_TOML} to a free slug (lowercase letters, digits and dashes); when it is empty, the slug is derived from the pack name"
            ),
        ),
        StatusCode::BAD_REQUEST => Error::usage(what, format!("fix {DIR}/ accordingly")),
        _ => Error::internal(
            what,
            "retry later; if it persists, check https://status.modrinth.com",
        ),
    })
}

fn read(response: Response<Body>, path: &str) -> Result<Remote> {
    check(response, path)?
        .body_mut()
        .read_json()
        .context(format!("reading the answer of Modrinth to {path}"))
}

#[cfg(test)]
mod tests;
