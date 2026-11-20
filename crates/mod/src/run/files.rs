//! The files of a run directory the CLI writes: the kind directories it
//! keeps in step with what is asked, `options.txt` of the client and
//! `server.properties` of the dev server. Both property files keep the lines
//! the CLI does not set, so that what the game or the player changed stays.

use std::collections::hash_map::RandomState;
use std::fs;
use std::hash::{BuildHasher, Hasher};
use std::path::Path;

use mc_core::error::Context as _;
use mc_core::model::entry::Kind;
use mc_core::model::write_file;

use super::Ports;
use crate::error::Result;

/// The game option of the accessibility onboarding screen.
const ONBOARDING: &str = "onboardAccessibility";

/// Extension of a file being downloaded, renamed once checked.
pub const PART: &str = "part";

/// Removes from `dir` the files of `kind` (by extension) not named in `keep`,
/// and the downloads an interrupted run left unfinished.
pub fn keep_only(dir: &Path, kind: Kind, keep: &[&str]) -> Result<()> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in entries {
        let path = entry.context(format!("reading {}", dir.display()))?.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let unfinished = path.extension().is_some_and(|e| e == PART);
        let unwanted = Kind::of_file_name(name) == Some(kind) && !keep.contains(&name);
        if path.is_file() && (unfinished || unwanted) {
            fs::remove_file(&path).context(format!("removing {}", path.display()))?;
        }
    }
    Ok(())
}

/// Writes the game options and the enabled resource packs into
/// `<dir>/options.txt`. The resource packs from files (`file/<name>`) become
/// `packs`, in order, after the packs the game lists (`vanilla`, the loader's).
/// The accessibility onboarding of a first launch is skipped unless the file
/// or `options` say otherwise: the game shows it before Quick Play, so the
/// client would wait there instead of joining the dev server.
pub fn write_options(dir: &Path, packs: &[&str], options: &[(String, String)]) -> Result<()> {
    let path = dir.join("options.txt");
    let text = fs::read_to_string(&path).unwrap_or_default();
    let mut set: Vec<(String, String)> = options.to_vec();
    if get(&text, ':', ONBOARDING).is_none() && !options.iter().any(|(k, _)| k == ONBOARDING) {
        set.insert(0, (ONBOARDING.to_owned(), "false".to_owned()));
    }
    let current = get(&text, ':', "resourcePacks");
    if current.is_some() || !packs.is_empty() {
        let mut list: Vec<String> = current
            .and_then(|v| serde_json::from_str(&v).ok())
            .unwrap_or_else(|| vec!["vanilla".to_owned()]);
        list.retain(|p| !p.starts_with("file/"));
        list.extend(packs.iter().map(|p| format!("file/{p}")));
        set.push((
            "resourcePacks".into(),
            serde_json::to_string(&list).expect("strings serialize"),
        ));
    }
    write_file(&path, &set_lines(&text, ':', &set))
}

/// Writes `<dir>/server.properties` for a dev server: on this machine only,
/// offline, without whitelist, with RCON. The RCON password is generated once
/// and kept.
pub fn write_server_properties(dir: &Path, ports: Ports) -> Result<()> {
    let path = dir.join("server.properties");
    let text = fs::read_to_string(&path).unwrap_or_default();
    let password = get(&text, '=', "rcon.password")
        .filter(|p| !p.is_empty())
        .unwrap_or_else(password);
    let set = [
        ("server-ip", "127.0.0.1".to_owned()),
        ("server-port", ports.game.to_string()),
        ("online-mode", "false".to_owned()),
        ("white-list", "false".to_owned()),
        ("enable-rcon", "true".to_owned()),
        ("rcon.port", ports.rcon.to_string()),
        ("rcon.password", password),
    ]
    .map(|(k, v)| (k.to_owned(), v));
    write_file(&path, &set_lines(&text, '=', &set))?;
    write_file(&dir.join("eula.txt"), "eula=true\n")
}

/// The RCON password of the server in `dir`.
pub fn rcon_password(dir: &Path) -> Option<String> {
    let text = fs::read_to_string(dir.join("server.properties")).ok()?;
    get(&text, '=', "rcon.password").filter(|p| !p.is_empty())
}

/// The value of `key` in a `key<sep>value` file.
fn get(text: &str, sep: char, key: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let (k, v) = line.split_once(sep)?;
        (k == key).then(|| v.to_owned())
    })
}

/// `text` with each key of `set` given its value: in place when the key is
/// there, appended otherwise.
fn set_lines(text: &str, sep: char, set: &[(String, String)]) -> String {
    let mut done = vec![false; set.len()];
    let mut out = String::new();
    for line in text.lines() {
        let key = line.split_once(sep).map(|(k, _)| k);
        match set.iter().position(|(k, _)| Some(k.as_str()) == key) {
            Some(i) => {
                if !done[i] {
                    out.push_str(&format!("{}{sep}{}\n", set[i].0, set[i].1));
                    done[i] = true;
                }
            }
            None => {
                out.push_str(line);
                out.push('\n');
            }
        }
    }
    for (i, (key, value)) in set.iter().enumerate() {
        if !done[i] {
            out.push_str(&format!("{key}{sep}{value}\n"));
        }
    }
    out
}

/// 32 hexadecimal digits from the standard library's randomly keyed hasher.
/// The server listens on this machine only.
fn password() -> String {
    (0..2)
        .map(|_| {
            let mut hasher = RandomState::new().build_hasher();
            hasher.write_u8(0);
            format!("{:016x}", hasher.finish())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_lines_replaces_in_place_and_appends() {
        let text = "a:1\nb:2\na:3\n";
        let set = [
            ("a".to_owned(), "9".to_owned()),
            ("c".to_owned(), "x".to_owned()),
        ];
        assert_eq!(set_lines(text, ':', &set), "a:9\nb:2\nc:x\n");
    }

    #[test]
    fn options_enable_packs_after_the_game_ones() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("options.txt");
        fs::write(
            &path,
            "version:4700\nresourcePacks:[\"vanilla\",\"fabric\",\"file/old.zip\"]\nlang:en_us\n",
        )
        .unwrap();
        let options = [("lang".to_owned(), "fr_fr".to_owned())];
        write_options(dir.path(), &["a.zip", "b.zip"], &options).unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "version:4700\nresourcePacks:[\"vanilla\",\"fabric\",\"file/a.zip\",\"file/b.zip\"]\nlang:fr_fr\n\
             onboardAccessibility:false\n"
        );
        write_options(dir.path(), &[], &[]).unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "version:4700\nresourcePacks:[\"vanilla\",\"fabric\"]\nlang:fr_fr\nonboardAccessibility:false\n"
        );
    }

    /// The accessibility onboarding screen of a first launch comes before
    /// Quick Play: the client would wait there instead of joining the server.
    #[test]
    fn options_skip_the_accessibility_onboarding() {
        let dir = tempfile::tempdir().unwrap();
        write_options(dir.path(), &[], &[]).unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join("options.txt")).unwrap(),
            "onboardAccessibility:false\n"
        );
        fs::remove_file(dir.path().join("options.txt")).unwrap();
        write_options(dir.path(), &["a.zip"], &[]).unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join("options.txt")).unwrap(),
            "onboardAccessibility:false\nresourcePacks:[\"vanilla\",\"file/a.zip\"]\n"
        );

        // The player's or the profile's choice stays.
        fs::write(
            dir.path().join("options.txt"),
            "onboardAccessibility:true\n",
        )
        .unwrap();
        write_options(dir.path(), &[], &[]).unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join("options.txt")).unwrap(),
            "onboardAccessibility:true\n"
        );
        let profile = [("onboardAccessibility".to_owned(), "true".to_owned())];
        fs::remove_file(dir.path().join("options.txt")).unwrap();
        write_options(dir.path(), &[], &profile).unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join("options.txt")).unwrap(),
            "onboardAccessibility:true\n"
        );
    }

    #[test]
    fn server_properties_keep_the_password_and_other_lines() {
        let dir = tempfile::tempdir().unwrap();
        let ports = Ports {
            game: 25565,
            rcon: 25575,
        };
        write_server_properties(dir.path(), ports).unwrap();
        let path = dir.path().join("server.properties");
        let first = fs::read_to_string(&path).unwrap();
        for line in [
            "server-ip=127.0.0.1",
            "server-port=25565",
            "online-mode=false",
            "white-list=false",
            "enable-rcon=true",
            "rcon.port=25575",
        ] {
            assert!(first.lines().any(|l| l == line), "{line} in {first}");
        }
        let password = get(&first, '=', "rcon.password").unwrap();
        assert_eq!(password.len(), 32);
        assert_ne!(password, super::password());

        fs::write(&path, format!("{first}motd=Mine\n")).unwrap();
        write_server_properties(
            dir.path(),
            Ports {
                game: 4000,
                rcon: 4001,
            },
        )
        .unwrap();
        let second = fs::read_to_string(&path).unwrap();
        assert_eq!(get(&second, '=', "rcon.password").unwrap(), password);
        assert_eq!(get(&second, '=', "server-port").as_deref(), Some("4000"));
        assert_eq!(get(&second, '=', "motd").as_deref(), Some("Mine"));
        assert_eq!(
            fs::read_to_string(dir.path().join("eula.txt")).unwrap(),
            "eula=true\n"
        );
    }

    #[test]
    fn keep_only_leaves_other_kinds_and_folders() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["a.jar", "b.jar", "c.zip", "notes.txt"] {
            fs::write(dir.path().join(name), b"x").unwrap();
        }
        fs::create_dir(dir.path().join("d.jar")).unwrap();
        keep_only(dir.path(), Kind::Mod, &["a.jar"]).unwrap();
        let mut left: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        left.sort();
        assert_eq!(left, ["a.jar", "c.zip", "d.jar", "notes.txt"]);
    }

    /// A download the CLI did not finish (interrupted) left its `.part`.
    #[test]
    fn keep_only_removes_unfinished_downloads() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["a.jar", "b-1.0.part"] {
            fs::write(dir.path().join(name), b"x").unwrap();
        }
        keep_only(dir.path(), Kind::Mod, &["a.jar"]).unwrap();
        assert!(!dir.path().join("b-1.0.part").exists());
        assert!(dir.path().join("a.jar").exists());
    }
}
