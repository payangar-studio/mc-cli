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
    server
        .mock("GET", REPO)
        .match_header("authorization", auth())
        .with_status(200)
        .with_body(
            json!({ "full_name": "payangar-studio/swift-blades", "private": private }).to_string(),
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

fn never_asked(name: &str, _: &str) -> Result<String> {
    panic!("asked for {name}")
}

#[test]
fn remote_urls_name_the_repository() {
    for url in [
        "https://github.com/payangar-studio/swift-blades.git",
        "https://github.com/payangar-studio/swift-blades",
        "git@github.com:payangar-studio/swift-blades.git",
        "ssh://git@github.com/payangar-studio/swift-blades.git\n",
    ] {
        assert_eq!(Repo::from_remote(url), Some(repo()), "{url}");
    }
    for url in [
        "https://gitlab.com/payangar-studio/swift-blades.git",
        "https://github.com/payangar-studio",
        "git@github.com:a/b/c.git",
        "",
    ] {
        assert_eq!(Repo::from_remote(url), None, "{url}");
    }
}

#[test]
fn a_new_repository_gets_the_variables_the_ruleset_and_codeowners() {
    let mut server = Server::new();
    let tmp = tempfile::tempdir().unwrap();
    let _repo = serve_repo(&mut server, false);
    let url = create_variable(&mut server, "INFISICAL_URL", "https://vault.example");
    let id = create_variable(&mut server, "INFISICAL_IDENTITY_ID", "identity-1");
    let list = no_rulesets(&mut server);
    let create = server
        .mock("POST", format!("{REPO}/rulesets").as_str())
        .match_header("authorization", auth())
        .match_body(Matcher::Json(release_tags_ruleset()))
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
            "Set the variable INFISICAL_URL.",
            "Set the variable INFISICAL_IDENTITY_ID.",
            "Created the ruleset `Release tags`.",
            "Wrote .github/CODEOWNERS with @maintainer as the owner of the workflows: commit it.",
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
    let ruleset = release_tags_ruleset();
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
fn a_second_run_updates_in_place_and_keeps_what_is_set() {
    let mut server = Server::new();
    let tmp = tempfile::tempdir().unwrap();
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
    let put = server
        .mock("PUT", format!("{REPO}/rulesets/42").as_str())
        .match_body(Matcher::Json(release_tags_ruleset()))
        .with_status(200)
        .with_body(r#"{"id":42}"#)
        .create();
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
            "Set the variable INFISICAL_URL.",
            "Kept the variable INFISICAL_IDENTITY_ID.",
            "Updated the ruleset `Release tags`.",
            "Kept .github/CODEOWNERS.",
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
    let tmp = tempfile::tempdir().unwrap();
    let _repo = serve_repo(&mut server, true);
    let _missing = [
        no_variable(&mut server, "INFISICAL_URL"),
        no_variable(&mut server, "INFISICAL_IDENTITY_ID"),
    ];
    let _url = create_variable(&mut server, "INFISICAL_URL", "https://asked.example");
    let _id = create_variable(&mut server, "INFISICAL_IDENTITY_ID", "asked-id");
    let _user = serve_user(&mut server);
    let rulesets = server
        .mock("GET", Matcher::Regex("/rulesets".into()))
        .expect(0)
        .create();
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
    // Private: no ruleset, a line says so.
    rulesets.assert();
    assert!(
        done.iter().any(|l| l.contains("the repository is private")),
        "{done:?}"
    );
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
    let tmp = tempfile::tempdir().unwrap();
    let _repo = serve_repo(&mut server, false);
    let _patch = server
        .mock("PATCH", Matcher::Regex("/actions/variables/".into()))
        .with_status(204)
        .create();
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
