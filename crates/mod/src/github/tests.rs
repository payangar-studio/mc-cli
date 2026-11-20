//! The GitHub setup against a mocked API.

use mockito::{Matcher, Server, ServerGuard};

use super::*;
use crate::error::ErrorKind;

const TOKEN: &str = "test-token";
const REPO: &str = "/repos/payangar-studio/swift-blades";

fn repo() -> Repo {
    Repo::from_remote("https://github.com/payangar-studio/swift-blades.git").unwrap()
}

fn client(server: &ServerGuard) -> GitHub {
    GitHub::new(&server.url(), TOKEN.into())
}

fn auth() -> Matcher {
    Matcher::Exact(format!("Bearer {TOKEN}"))
}

fn serve_repo(server: &mut ServerGuard, private: bool) -> mockito::Mock {
    serve_repo_of(server, private, "Organization")
}

fn serve_repo_of(server: &mut ServerGuard, private: bool, owner: &str) -> mockito::Mock {
    serve_repo_with(server, private, owner, "26.3")
}

fn serve_repo_with(
    server: &mut ServerGuard,
    private: bool,
    owner: &str,
    default_branch: &str,
) -> mockito::Mock {
    server
        .mock("GET", REPO)
        .match_header("authorization", auth())
        .with_status(200)
        .with_body(
            json!({
                "full_name": "payangar-studio/swift-blades",
                "private": private,
                "owner": { "login": "payangar-studio", "type": owner },
                "default_branch": default_branch,
            })
            .to_string(),
        )
        .create()
}

/// Every variable PATCH succeeds.
fn any_variable(server: &mut ServerGuard) -> mockito::Mock {
    server
        .mock("PATCH", Matcher::Regex("/actions/variables/".into()))
        .with_status(204)
        .create()
}

/// The answer GitHub gives to rulesets on a private repository of a free plan.
fn rulesets_need_a_plan(server: &mut ServerGuard) -> mockito::Mock {
    server
        .mock("GET", format!("{REPO}/rulesets").as_str())
        .match_query(Matcher::Any)
        .with_status(403)
        .with_body(
            r#"{"message":"Upgrade to GitHub Pro or make this repository public to enable this feature.","status":"403"}"#,
        )
        .create()
}

fn serve_user(server: &mut ServerGuard) -> mockito::Mock {
    server
        .mock("GET", "/user")
        .match_header("authorization", auth())
        .with_status(200)
        .with_body(r#"{"login":"maintainer"}"#)
        .create()
}

fn no_variable(server: &mut ServerGuard, name: &str) -> mockito::Mock {
    let path = format!("{REPO}/actions/variables/{name}");
    server
        .mock("GET", path.as_str())
        .with_status(404)
        .with_body(r#"{"message":"Not Found"}"#)
        .create()
}

/// PATCH answers 404 (the variable is missing), then POST creates it with
/// `value`.
fn create_variable(server: &mut ServerGuard, name: &str, value: &str) -> [mockito::Mock; 2] {
    let path = format!("{REPO}/actions/variables/{name}");
    let body = json!({ "name": name, "value": value });
    [
        server
            .mock("PATCH", path.as_str())
            .match_header("authorization", auth())
            .match_body(Matcher::Json(body.clone()))
            .with_status(404)
            .with_body(r#"{"message":"Not Found"}"#)
            .create(),
        server
            .mock("POST", format!("{REPO}/actions/variables").as_str())
            .match_header("authorization", auth())
            .match_body(Matcher::Json(body))
            .with_status(201)
            .with_body("{}")
            .create(),
    ]
}

fn no_rulesets(server: &mut ServerGuard) -> mockito::Mock {
    server
        .mock("GET", format!("{REPO}/rulesets").as_str())
        .match_query(Matcher::UrlEncoded(
            "includes_parents".into(),
            "false".into(),
        ))
        .match_header("authorization", auth())
        .with_status(200)
        .with_body("[]")
        .create()
}

/// [`super::configure`], with the lines it reports as its result.
fn configure(
    gh: &GitHub,
    repo: &Repo,
    root: &Path,
    given: [Option<String>; 2],
    ask: impl FnMut(&str, &str) -> Result<String>,
) -> Result<Vec<String>> {
    let mut done = Vec::new();
    super::configure(gh, repo, root, given, ask, &mut done).map(|()| done)
}

fn never_asked(name: &str, _: &str) -> Result<String> {
    panic!("asked for {name}")
}

const SITE: &str = "https://payangar-studio.github.io/swift-blades/";

/// Pages already publishes `design` from `/docs`.
fn pages_on_design(server: &mut ServerGuard) -> mockito::Mock {
    server
        .mock("GET", format!("{REPO}/pages").as_str())
        .match_header("authorization", auth())
        .with_status(200)
        .with_body(
            json!({
                "build_type": "legacy",
                "source": { "branch": "design", "path": "/docs" },
                "html_url": SITE,
            })
            .to_string(),
        )
        .create()
}

fn kept_pages() -> String {
    format!("Kept GitHub Pages on the `design` branch, folder `/docs`: {SITE}design/")
}

/// A checkout of the mod, on branch `26.3`, and its `origin`: a local bare
/// repository.
struct Checkout {
    dir: tempfile::TempDir,
    origin: tempfile::TempDir,
}

impl Checkout {
    fn path(&self) -> &Path {
        self.dir.path()
    }
}

fn git(dir: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args([
            "-c",
            "user.name=Tester",
            "-c",
            "user.email=tester@example.invalid",
        ])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {output:?}");
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// A checkout with the branches `pushed` on its origin and `local` here only.
fn checkout_with(pushed: &[&str], local: &[&str]) -> Checkout {
    let dir = tempfile::tempdir().unwrap();
    let origin = tempfile::tempdir().unwrap();
    git(origin.path(), &["init", "--quiet", "--bare"]);
    let root = dir.path();
    git(root, &["init", "--quiet", "--initial-branch=26.3"]);
    git(root, &["commit", "--quiet", "--allow-empty", "-m", "start"]);
    git(
        root,
        &["remote", "add", "origin", origin.path().to_str().unwrap()],
    );
    for branch in pushed.iter().chain(local) {
        if *branch != "26.3" {
            git(root, &["branch", branch]);
        }
    }
    for branch in pushed {
        git(root, &["push", "--quiet", "origin", branch]);
    }
    Checkout { dir, origin }
}

/// The usual state of a second run: `26.3` and `design` on origin.
fn checkout() -> Checkout {
    checkout_with(&["26.3", "design"], &[])
}

#[test]
fn remote_urls_name_the_repository() {
    for url in [
        "https://github.com/payangar-studio/swift-blades.git",
        "https://github.com/payangar-studio/swift-blades",
        "git@github.com:payangar-studio/swift-blades.git",
        "ssh://git@github.com/payangar-studio/swift-blades.git\n",
        "https://user@github.com/payangar-studio/swift-blades.git",
        "https://user:secret@github.com/payangar-studio/swift-blades",
        "http://github.com/payangar-studio/swift-blades.git",
        "ssh://git@github.com:22/payangar-studio/swift-blades.git",
        "https://GitHub.com/payangar-studio/swift-blades.git",
        "github.com:payangar-studio/swift-blades.git",
    ] {
        assert_eq!(Repo::from_remote(url), Some(repo()), "{url}");
    }
    for url in [
        "https://gitlab.com/payangar-studio/swift-blades.git",
        "https://github.com/payangar-studio",
        "git@github.com:a/b/c.git",
        "https://github.com.evil.example/payangar-studio/swift-blades.git",
        "git@github-work:payangar-studio/swift-blades.git",
        "/home/me/swift-blades",
        "",
    ] {
        assert_eq!(Repo::from_remote(url), None, "{url}");
    }
}

#[test]
fn a_new_repository_gets_the_variables_the_ruleset_and_codeowners() {
    let mut server = Server::new();
    let tmp = checkout();
    let _pages = pages_on_design(&mut server);
    let _repo = serve_repo(&mut server, false);
    let url = create_variable(&mut server, "INFISICAL_URL", "https://vault.example");
    let id = create_variable(&mut server, "INFISICAL_IDENTITY_ID", "identity-1");
    let list = no_rulesets(&mut server);
    let create = server
        .mock("POST", format!("{REPO}/rulesets").as_str())
        .match_header("authorization", auth())
        .match_body(Matcher::Json(release_tags_ruleset(true)))
        .with_status(201)
        .with_body(r#"{"id":42}"#)
        .create();
    let user = serve_user(&mut server);

    let done = configure(
        &client(&server),
        &repo(),
        tmp.path(),
        [
            Some("https://vault.example".into()),
            Some("identity-1".into()),
        ],
        never_asked,
    )
    .unwrap();

    for mock in url.iter().chain(&id).chain([&list, &create, &user]) {
        mock.assert();
    }
    assert_eq!(
        done,
        [
            "Set the variable INFISICAL_URL.".to_owned(),
            "Set the variable INFISICAL_IDENTITY_ID.".into(),
            "Created the ruleset `Release tags`.".into(),
            "Wrote .github/CODEOWNERS with @maintainer as the owner of the workflows: commit it."
                .into(),
            "Kept the default branch 26.3.".into(),
            kept_pages(),
        ]
    );
    let owners = std::fs::read_to_string(tmp.path().join(CODEOWNERS)).unwrap();
    assert!(
        owners.contains("/.github/workflows/ @maintainer\n"),
        "{owners}"
    );
}

#[test]
fn the_ruleset_reserves_v_tags_to_admins() {
    let ruleset = release_tags_ruleset(true);
    assert_eq!(ruleset["target"], "tag");
    assert_eq!(ruleset["enforcement"], "active");
    assert_eq!(
        ruleset["conditions"]["ref_name"]["include"],
        json!(["refs/tags/v*"])
    );
    let rules: Vec<_> = ruleset["rules"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["type"].as_str().unwrap())
        .collect();
    assert_eq!(rules, ["creation", "update", "deletion"]);
    let actors: Vec<_> = ruleset["bypass_actors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["actor_type"].as_str().unwrap())
        .collect();
    assert_eq!(actors, ["RepositoryRole", "OrganizationAdmin"]);
}

#[test]
fn a_user_repository_gets_no_organization_admin_bypass() {
    let mut server = Server::new();
    let tmp = checkout();
    let _pages = pages_on_design(&mut server);
    let _repo = serve_repo_of(&mut server, false, "User");
    let _vars = any_variable(&mut server);
    let _list = no_rulesets(&mut server);
    let create = server
        .mock("POST", format!("{REPO}/rulesets").as_str())
        .match_body(Matcher::Json(release_tags_ruleset(false)))
        .with_status(201)
        .with_body(r#"{"id":42}"#)
        .create();
    let _user = serve_user(&mut server);

    configure(
        &client(&server),
        &repo(),
        tmp.path(),
        [Some("u".into()), Some("i".into())],
        never_asked,
    )
    .unwrap();

    create.assert();
    let actors = release_tags_ruleset(false)["bypass_actors"].clone();
    assert_eq!(
        actors,
        json!([{ "actor_type": "RepositoryRole", "actor_id": 5, "bypass_mode": "always" }])
    );
}

#[test]
fn a_second_run_keeps_what_is_set() {
    let mut server = Server::new();
    let tmp = checkout();
    let _pages = pages_on_design(&mut server);
    std::fs::create_dir_all(tmp.path().join(".github")).unwrap();
    std::fs::write(tmp.path().join(CODEOWNERS), "* @someone\n").unwrap();
    let _repo = serve_repo(&mut server, false);
    // The URL is given: it is updated in place.
    let patch = server
        .mock(
            "PATCH",
            format!("{REPO}/actions/variables/INFISICAL_URL").as_str(),
        )
        .match_body(Matcher::Json(
            json!({ "name": "INFISICAL_URL", "value": "https://vault.example" }),
        ))
        .with_status(204)
        .create();
    // The identity is not given and already set: it is kept.
    let get = server
        .mock(
            "GET",
            format!("{REPO}/actions/variables/INFISICAL_IDENTITY_ID").as_str(),
        )
        .with_status(200)
        .with_body(r#"{"name":"INFISICAL_IDENTITY_ID","value":"identity-1"}"#)
        .create();
    let list = server
        .mock("GET", format!("{REPO}/rulesets").as_str())
        .match_query(Matcher::Any)
        .with_status(200)
        .with_body(r#"[{"id":7,"name":"Something else"},{"id":42,"name":"Release tags"}]"#)
        .create();
    // The ruleset exists: an admin may have changed it, it is left alone.
    let put = server.mock("PUT", Matcher::Any).expect(0).create();
    let post = server.mock("POST", Matcher::Any).expect(0).create();

    let done = configure(
        &client(&server),
        &repo(),
        tmp.path(),
        [Some("https://vault.example".into()), None],
        never_asked,
    )
    .unwrap();

    for mock in [&patch, &get, &list, &put, &post] {
        mock.assert();
    }
    assert_eq!(
        done,
        [
            "Set the variable INFISICAL_URL.".to_owned(),
            "Kept the variable INFISICAL_IDENTITY_ID.".into(),
            "Kept the ruleset `Release tags`.".into(),
            "Kept .github/CODEOWNERS.".into(),
            "Kept the default branch 26.3.".into(),
            kept_pages(),
        ]
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join(CODEOWNERS)).unwrap(),
        "* @someone\n"
    );
}

#[test]
fn a_missing_value_is_asked_for() {
    let mut server = Server::new();
    let tmp = checkout();
    let _pages = pages_on_design(&mut server);
    let _repo = serve_repo(&mut server, true);
    let _missing = [
        no_variable(&mut server, "INFISICAL_URL"),
        no_variable(&mut server, "INFISICAL_IDENTITY_ID"),
    ];
    let _url = create_variable(&mut server, "INFISICAL_URL", "https://asked.example");
    let _id = create_variable(&mut server, "INFISICAL_IDENTITY_ID", "asked-id");
    let _user = serve_user(&mut server);
    let rulesets = rulesets_need_a_plan(&mut server);
    let mut asked = Vec::new();

    let done = configure(
        &client(&server),
        &repo(),
        tmp.path(),
        [None, None],
        |name, _| {
            asked.push(name.to_owned());
            Ok(match name {
                "INFISICAL_URL" => "https://asked.example".into(),
                _ => "asked-id".into(),
            })
        },
    )
    .unwrap();

    assert_eq!(asked, ["INFISICAL_URL", "INFISICAL_IDENTITY_ID"]);
    // Private on a free plan: GitHub refuses rulesets, a line says so.
    rulesets.assert();
    assert!(
        done.iter().any(|l| l.contains("needs a paid plan")),
        "{done:?}"
    );
}

#[test]
fn a_private_repository_on_a_paid_plan_gets_the_ruleset() {
    let mut server = Server::new();
    let tmp = checkout();
    let _pages = pages_on_design(&mut server);
    let _repo = serve_repo(&mut server, true);
    let _vars = any_variable(&mut server);
    let _list = no_rulesets(&mut server);
    let create = server
        .mock("POST", format!("{REPO}/rulesets").as_str())
        .with_status(201)
        .with_body(r#"{"id":42}"#)
        .create();
    let _user = serve_user(&mut server);

    let done = configure(
        &client(&server),
        &repo(),
        tmp.path(),
        [Some("u".into()), Some("i".into())],
        never_asked,
    )
    .unwrap();

    create.assert();
    assert!(
        done.contains(&"Created the ruleset `Release tags`.".to_owned()),
        "{done:?}"
    );
}

#[test]
fn a_bad_value_stops_setup_before_any_change() {
    for given in [
        [Some("https://vault.example".into()), Some(" ".into())],
        [Some("https://vault.example".into()), None],
    ] {
        let mut server = Server::new();
        let tmp = checkout();
        let _repo = serve_repo(&mut server, false);
        let _missing = no_variable(&mut server, "INFISICAL_IDENTITY_ID");
        let writes = server.mock("PATCH", Matcher::Any).expect(0).create();
        let creates = server.mock("POST", Matcher::Any).expect(0).create();

        // Without a terminal, a missing value is a usage error.
        let err = configure(&client(&server), &repo(), tmp.path(), given, |name, _| {
            Err(Error::usage(format!("missing {name}"), "pass it"))
        })
        .unwrap_err();

        assert_eq!(err.kind(), ErrorKind::Usage);
        writes.assert();
        creates.assert();
        assert!(!tmp.path().join(CODEOWNERS).exists());
    }
}

#[test]
fn a_codeowners_elsewhere_is_kept() {
    for path in ["CODEOWNERS", "docs/CODEOWNERS"] {
        let mut server = Server::new();
        let tmp = checkout();
        let _pages = pages_on_design(&mut server);
        let file = tmp.path().join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "* @team\n").unwrap();
        let _repo = serve_repo(&mut server, false);
        let _vars = any_variable(&mut server);
        let _list = server
            .mock("GET", format!("{REPO}/rulesets").as_str())
            .match_query(Matcher::Any)
            .with_status(200)
            .with_body(r#"[{"id":42,"name":"Release tags"}]"#)
            .create();

        let done = configure(
            &client(&server),
            &repo(),
            tmp.path(),
            [Some("u".into()), Some("i".into())],
            never_asked,
        )
        .unwrap();

        assert!(
            !tmp.path().join(CODEOWNERS).exists(),
            "{path}: .github/CODEOWNERS would override it"
        );
        assert!(done.contains(&format!("Kept {path}.")), "{done:?}");
    }
}

#[test]
fn a_missing_repository_is_left_alone() {
    let mut server = Server::new();
    let tmp = tempfile::tempdir().unwrap();
    let _repo = server.mock("GET", REPO).with_status(404).create();
    let others = server.mock("PATCH", Matcher::Any).expect(0).create();

    let done = configure(
        &client(&server),
        &repo(),
        tmp.path(),
        [None, None],
        never_asked,
    )
    .unwrap();

    others.assert();
    assert!(done[0].contains("does not exist on GitHub"), "{done:?}");
    assert!(!tmp.path().join(CODEOWNERS).exists());
}

#[test]
fn a_refused_token_points_to_gh_auth() {
    let mut server = Server::new();
    let tmp = tempfile::tempdir().unwrap();
    let _repo = server
        .mock("GET", REPO)
        .with_status(401)
        .with_body(r#"{"message":"Bad credentials"}"#)
        .create();

    let err = configure(
        &client(&server),
        &repo(),
        tmp.path(),
        [None, None],
        never_asked,
    )
    .unwrap_err();

    assert_eq!(err.kind(), ErrorKind::Usage);
    assert!(err.hint().contains("gh auth login"), "{}", err.hint());
}

#[test]
fn a_ruleset_refused_names_the_answer() {
    let mut server = Server::new();
    let tmp = checkout();
    let _repo = serve_repo(&mut server, false);
    let _patch = any_variable(&mut server);
    let _list = no_rulesets(&mut server);
    let _create = server
        .mock("POST", format!("{REPO}/rulesets").as_str())
        .with_status(422)
        .with_body(r#"{"message":"Validation Failed"}"#)
        .create();

    let err = configure(
        &client(&server),
        &repo(),
        tmp.path(),
        [Some("u".into()), Some("i".into())],
        never_asked,
    )
    .unwrap_err();

    assert!(err.to_string().contains("422"), "{err}");
    assert!(err.to_string().contains("Validation Failed"), "{err}");
}

/// Every step before the design one, as on a second run.
fn settled(server: &mut ServerGuard, private: bool, default_branch: &str) -> Vec<mockito::Mock> {
    vec![
        serve_repo_with(server, private, "Organization", default_branch),
        any_variable(server),
        server
            .mock("GET", format!("{REPO}/rulesets").as_str())
            .match_query(Matcher::Any)
            .with_status(200)
            .with_body(r#"[{"id":42,"name":"Release tags"}]"#)
            .create(),
        serve_user(server),
    ]
}

fn run(server: &ServerGuard, checkout: &Checkout) -> Result<Vec<String>> {
    configure(
        &client(server),
        &repo(),
        checkout.path(),
        [Some("u".into()), Some("i".into())],
        never_asked,
    )
}

/// On an empty repository the first branch pushed becomes the default: the
/// version branch must be there before `design`.
#[test]
fn a_version_branch_missing_on_origin_stops_setup_before_any_change() {
    for (pushed, local, missing) in [
        (&[][..], &["design"][..], "26.3"),
        (&["26.3"][..], &["26.4", "design"][..], "26.4"),
    ] {
        let mut server = Server::new();
        let tmp = checkout_with(pushed, local);
        let _repo = serve_repo(&mut server, false);
        let changes: Vec<_> = ["PATCH", "POST", "PUT"]
            .map(|m| server.mock(m, Matcher::Any).expect(0).create())
            .into();

        let err = run(&server, &tmp).unwrap_err();

        assert_eq!(err.kind(), ErrorKind::Usage);
        assert!(
            err.to_string()
                .contains(&format!("the branch {missing} is not on origin")),
            "{err}"
        );
        assert!(
            err.hint()
                .contains(&format!("git push -u origin {missing}")),
            "{}",
            err.hint()
        );
        for mock in &changes {
            mock.assert();
        }
        assert!(!tmp.path().join(CODEOWNERS).exists());
        assert_eq!(
            git(tmp.origin.path(), &["for-each-ref", "refs/heads/design"]),
            ""
        );
    }
}

/// The state the live check found: `design` pushed first became the default.
#[test]
fn the_first_run_sets_the_default_branch_pushes_design_and_creates_the_site() {
    let mut server = Server::new();
    let tmp = checkout_with(&["26.10", "26.3"], &["design"]);
    let _settled = settled(&mut server, false, "design");
    let default = server
        .mock("PATCH", REPO)
        .match_header("authorization", auth())
        .match_body(Matcher::Json(json!({ "default_branch": "26.10" })))
        .with_status(200)
        .with_body("{}")
        .create();
    let read = server
        .mock("GET", format!("{REPO}/pages").as_str())
        .with_status(404)
        .with_body(r#"{"message":"Not Found"}"#)
        .create();
    let create = server
        .mock("POST", format!("{REPO}/pages").as_str())
        .match_header("authorization", auth())
        .match_body(Matcher::Json(json!({
            "build_type": "legacy",
            "source": { "branch": "design", "path": "/docs" },
        })))
        .with_status(201)
        .with_body(json!({ "html_url": SITE }).to_string())
        .create();

    let done = run(&server, &tmp).unwrap();

    for mock in [&default, &read, &create] {
        mock.assert();
    }
    assert_eq!(
        git(tmp.origin.path(), &["rev-parse", "design"]),
        git(tmp.path(), &["rev-parse", "design"])
    );
    assert_eq!(
        done[done.len() - 3..],
        [
            "Set the default branch to 26.10.".to_owned(),
            "Pushed the `design` branch.".into(),
            format!(
                "Set GitHub Pages to the `design` branch, folder `/docs`: {SITE}design/ (built within a few minutes)."
            ),
        ]
    );
}

/// A site set up for something else is left alone, like an existing ruleset
/// or `CODEOWNERS`: a line says what it publishes.
#[test]
fn a_site_from_another_source_is_kept() {
    for (build_type, branch, path, source) in [
        ("legacy", "26.3", "/", "the `26.3` branch, folder `/`"),
        ("legacy", "design", "/", "the `design` branch, folder `/`"),
        ("workflow", "design", "/docs", "a workflow"),
    ] {
        let mut server = Server::new();
        let tmp = checkout();
        let _settled = settled(&mut server, false, "26.3");
        let _read = server
            .mock("GET", format!("{REPO}/pages").as_str())
            .with_status(200)
            .with_body(
                json!({
                    "build_type": build_type,
                    "source": { "branch": branch, "path": path },
                    "html_url": SITE,
                })
                .to_string(),
            )
            .create();
        let changes: Vec<_> = ["PUT", "POST"]
            .map(|m| {
                server
                    .mock(m, format!("{REPO}/pages").as_str())
                    .expect(0)
                    .create()
            })
            .into();

        let done = run(&server, &tmp).unwrap();

        for mock in &changes {
            mock.assert();
        }
        assert_eq!(
            done.last().unwrap(),
            &format!(
                "Kept GitHub Pages, which publishes {source}: the design page is not published. To publish it, set the source of Pages to the `design` branch, folder `/docs`, in the settings of the repository."
            ),
        );
    }
}

/// A Pages site is public on the internet, even when its repository is
/// private: the design of a private mod is not published.
#[test]
fn a_private_repository_gets_no_pages_site() {
    let mut server = Server::new();
    let tmp = checkout_with(&["26.3"], &["design"]);
    let _settled = settled(&mut server, true, "26.3");
    let pages = server
        .mock("GET", format!("{REPO}/pages").as_str())
        .expect(0)
        .create();
    let create = server
        .mock("POST", format!("{REPO}/pages").as_str())
        .expect(0)
        .create();

    let done = run(&server, &tmp).unwrap();

    pages.assert();
    create.assert();
    assert_eq!(
        done[done.len() - 2..],
        [
            "Pushed the `design` branch.".to_owned(),
            "Skipped GitHub Pages: payangar-studio/swift-blades is private, and a Pages site is public on the internet whatever its repository. To publish the design page anyway, set the source of Pages to the `design` branch, folder `/docs`, in the settings of the repository.".into(),
        ]
    );
}

/// On a public repository the same refusal is an error: it names the answer.
#[test]
fn a_site_refused_on_a_public_repository_names_the_answer() {
    let mut server = Server::new();
    let tmp = checkout();
    let _settled = settled(&mut server, false, "26.3");
    let _read = server
        .mock("GET", format!("{REPO}/pages").as_str())
        .with_status(404)
        .create();
    let _create = server
        .mock("POST", format!("{REPO}/pages").as_str())
        .with_status(422)
        .with_body(r#"{"message":"Validation Failed"}"#)
        .create();

    let err = run(&server, &tmp).unwrap_err();

    assert!(err.to_string().contains("422"), "{err}");
    assert!(err.to_string().contains("Validation Failed"), "{err}");
}

#[test]
fn without_a_design_branch_pages_is_skipped() {
    let mut server = Server::new();
    let tmp = checkout_with(&["26.3"], &[]);
    let _settled = settled(&mut server, false, "26.3");
    let pages = server
        .mock("GET", format!("{REPO}/pages").as_str())
        .expect(0)
        .create();

    let done = run(&server, &tmp).unwrap();

    pages.assert();
    assert_eq!(
        done[done.len() - 2..],
        [
            "Kept the default branch 26.3.".to_owned(),
            "Skipped GitHub Pages: no `design` branch here or on origin.".into(),
        ]
    );
}

/// A repository on a branch that names no Minecraft version (`main`, kept by
/// `mod init` in an existing repository) still gets the rest of setup.
#[test]
fn without_a_version_branch_the_rest_of_setup_runs() {
    let mut server = Server::new();
    let tmp = checkout_with(&[], &["design"]);
    git(tmp.path(), &["branch", "-m", "26.3", "main"]);
    git(tmp.path(), &["push", "--quiet", "origin", "main"]);
    let _settled = settled(&mut server, false, "main");
    let changes: Vec<_> = [("PATCH", REPO.to_owned()), ("GET", format!("{REPO}/pages"))]
        .map(|(m, p)| server.mock(m, p.as_str()).expect(0).create())
        .into();

    let done = run(&server, &tmp).unwrap();

    for mock in &changes {
        mock.assert();
    }
    assert_eq!(done[0], "Set the variable INFISICAL_URL.");
    assert_eq!(
        done.last().unwrap(),
        "Skipped the default branch and GitHub Pages: no branch named after a Minecraft version (`26.3`) here or on origin."
    );
    assert_eq!(
        git(tmp.origin.path(), &["for-each-ref", "refs/heads/design"]),
        ""
    );
}

/// Setup runs without a terminal: git does not ask for a password, and a
/// remote it cannot read leaves out only the steps that need it.
#[test]
fn an_origin_git_cannot_read_leaves_the_rest_of_setup() {
    let mut server = Server::new();
    let tmp = checkout_with(&[], &["design"]);
    let url = format!("{}/payangar-studio/swift-blades.git", server.url());
    git(tmp.path(), &["remote", "set-url", "origin", &url]);
    // No credential helper nor askpass program of the machine answers.
    git(tmp.path(), &["config", "credential.helper", ""]);
    git(tmp.path(), &["config", "core.askPass", ""]);
    let _login = server
        .mock("GET", Matcher::Regex("/swift-blades.git/".into()))
        .with_status(401)
        .with_header("WWW-Authenticate", "Basic realm=\"GitHub\"")
        .create();
    let _settled = settled(&mut server, false, "26.3");
    let default = server.mock("PATCH", REPO).expect(0).create();

    let done = run(&server, &tmp).unwrap();

    default.assert();
    let last = done.last().unwrap();
    assert!(
        last.starts_with(
            "Skipped the default branch and GitHub Pages: could not read the branches of origin:"
        ),
        "{done:?}"
    );
    assert!(last.contains("terminal prompts disabled"), "{last}");
}

/// A failed step keeps the lines of the steps before it, among them the
/// reminder to commit `CODEOWNERS`.
#[test]
fn a_failed_step_keeps_what_the_steps_before_it_did() {
    let mut server = Server::new();
    let tmp = checkout_with(&["26.3"], &["design"]);
    let nowhere = tmp.path().join("nowhere.git");
    git(
        tmp.path(),
        &["config", "remote.origin.pushurl", nowhere.to_str().unwrap()],
    );
    let _settled = settled(&mut server, false, "26.3");
    let mut done = Vec::new();

    let err = super::configure(
        &client(&server),
        &repo(),
        tmp.path(),
        [Some("u".into()), Some("i".into())],
        never_asked,
        &mut done,
    )
    .unwrap_err();

    assert!(err.to_string().contains("could not push"), "{err}");
    assert_eq!(
        done,
        [
            "Set the variable INFISICAL_URL.".to_owned(),
            "Set the variable INFISICAL_IDENTITY_ID.".into(),
            "Kept the ruleset `Release tags`.".into(),
            "Wrote .github/CODEOWNERS with @maintainer as the owner of the workflows: commit it."
                .into(),
            "Kept the default branch 26.3.".into(),
        ]
    );
}

/// Without `html_url` in the answer, the line has no URL rather than a
/// relative one.
#[test]
fn a_site_without_its_url_is_reported_without_one() {
    let mut server = Server::new();
    let tmp = checkout();
    let _settled = settled(&mut server, false, "26.3");
    let _read = server
        .mock("GET", format!("{REPO}/pages").as_str())
        .with_status(404)
        .create();
    let _create = server
        .mock("POST", format!("{REPO}/pages").as_str())
        .with_status(201)
        .with_body("{}")
        .create();

    let done = run(&server, &tmp).unwrap();

    assert_eq!(
        done.last().unwrap(),
        "Set GitHub Pages to the `design` branch, folder `/docs` (built within a few minutes)."
    );
}
