//! The cases of the side matrix, derived from the side in `mod.toml`.

use crate::model::mod_toml::Side;

/// The client of a case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Client {
    /// No client: the server alone.
    None,
    /// The vanilla client: a player without the mod nor any loader.
    Vanilla,
    /// The loader's client with the mod and its dependencies.
    Modded,
}

/// What a case expects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expect {
    /// The server starts and keeps running.
    Runs,
    /// The client joins and is still connected some ticks later.
    Plays,
    /// The server refuses the client because it lacks the mod.
    Refused,
}

/// One case: a server with or without the mod, a client, what must happen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Case {
    /// Name on the command line (`--case`) and of its run directory.
    pub name: &'static str,
    /// What the case proves, for the report.
    pub what: &'static str,
    /// Whether the server has the mod and its dependencies; without, it has
    /// its loader only.
    pub server_has_mod: bool,
    /// Whether the server must load the mod: Fabric skips a client-only mod
    /// on a server, NeoForge loads its jar.
    pub server_loads_mod: bool,
    pub client: Client,
    pub expect: Expect,
}

const fn case(
    name: &'static str,
    what: &'static str,
    server_has_mod: bool,
    client: Client,
    expect: Expect,
) -> Case {
    Case {
        name,
        what,
        server_has_mod,
        server_loads_mod: server_has_mod,
        client,
        expect,
    }
}

const SERVER_STARTS: Case = case(
    "server-starts",
    "the dedicated server starts with the mod",
    true,
    Client::None,
    Expect::Runs,
);
const MODDED_CLIENT_JOINS: Case = case(
    "modded-client-joins",
    "a client with the mod joins a server with the mod",
    true,
    Client::Modded,
    Expect::Plays,
);
const MODDED_CLIENT_JOINS_UNMODDED_SERVER: Case = case(
    "modded-client-joins-unmodded-server",
    "a client with the mod joins a server without it",
    false,
    Client::Modded,
    Expect::Plays,
);
const VANILLA_CLIENT_JOINS: Case = case(
    "vanilla-client-joins",
    "a vanilla client joins a server with the mod and plays",
    true,
    Client::Vanilla,
    Expect::Plays,
);

/// The cases of a mod of side `side`, in the order they run.
pub fn cases(side: Side) -> Vec<Case> {
    match side {
        Side::Client => vec![
            Case {
                name: "server-holds-jar",
                what: "a dedicated server holding the jar does not break",
                server_loads_mod: false,
                ..SERVER_STARTS
            },
            Case {
                what: "the client works with the mod and joins a server without it",
                ..MODDED_CLIENT_JOINS_UNMODDED_SERVER
            },
        ],
        Side::Server => vec![SERVER_STARTS, VANILLA_CLIENT_JOINS],
        Side::BothRequired => vec![
            SERVER_STARTS,
            MODDED_CLIENT_JOINS,
            case(
                "vanilla-client-refused",
                "a vanilla client is refused with a clear message",
                true,
                Client::Vanilla,
                Expect::Refused,
            ),
        ],
        Side::BothOptional => vec![
            SERVER_STARTS,
            MODDED_CLIENT_JOINS,
            VANILLA_CLIENT_JOINS,
            MODDED_CLIENT_JOINS_UNMODDED_SERVER,
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(side: Side) -> Vec<&'static str> {
        cases(side).iter().map(|c| c.name).collect()
    }

    #[test]
    fn each_side_has_its_cases() {
        assert_eq!(
            names(Side::Client),
            ["server-holds-jar", "modded-client-joins-unmodded-server"]
        );
        assert_eq!(
            names(Side::Server),
            ["server-starts", "vanilla-client-joins"]
        );
        assert_eq!(
            names(Side::BothRequired),
            [
                "server-starts",
                "modded-client-joins",
                "vanilla-client-refused"
            ]
        );
        assert_eq!(
            names(Side::BothOptional),
            [
                "server-starts",
                "modded-client-joins",
                "vanilla-client-joins",
                "modded-client-joins-unmodded-server"
            ]
        );
    }

    #[test]
    fn only_a_mod_with_client_code_runs_a_modded_client() {
        assert!(
            cases(Side::Server)
                .iter()
                .all(|c| c.client != Client::Modded)
        );
    }

    #[test]
    fn a_client_only_mod_is_not_loaded_by_the_server() {
        for c in cases(Side::Client) {
            assert!(!c.server_loads_mod, "{c:?}");
        }
        for side in [Side::Server, Side::BothRequired, Side::BothOptional] {
            for c in cases(side) {
                assert_eq!(c.server_loads_mod, c.server_has_mod, "{c:?}");
            }
        }
    }

    #[test]
    fn only_a_mod_required_on_both_sides_refuses_a_client() {
        for side in Side::ALL {
            let refuses = cases(side).iter().any(|c| c.expect == Expect::Refused);
            assert_eq!(refuses, side == Side::BothRequired, "{side}");
        }
    }
}
