//! The side of a mod: the `side` of `mod.toml`, fixed at `mod init`. It is the
//! one source of what the platforms show about where the mod runs.

use serde::{Deserialize, Serialize};

use super::entry::{Env, Support};

/// Where a mod runs, as `mod.toml` spells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Side {
    /// On the client only; a server never needs it.
    Client,
    /// On the server only; a client joins without it.
    Server,
    /// On both: a client without it is refused by a server with it.
    BothRequired,
    /// On both, but a client without it still joins a server with it. The
    /// server is the side that needs it.
    BothOptional,
}

impl Side {
    pub const ALL: [Side; 4] = [
        Side::Client,
        Side::Server,
        Side::BothRequired,
        Side::BothOptional,
    ];

    /// What each side needs, in the platforms' terms (Modrinth's
    /// `client_side` and `server_side`).
    pub fn env(self) -> Env {
        use Support::{Optional, Required, Unsupported};
        let (client, server) = match self {
            Side::Client => (Required, Unsupported),
            Side::Server => (Unsupported, Required),
            Side::BothRequired => (Required, Required),
            Side::BothOptional => (Optional, Required),
        };
        Env { client, server }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_spelling_of_mod_toml() {
        #[derive(Deserialize)]
        struct ModToml {
            side: Side,
        }
        for (text, side) in [
            ("client", Side::Client),
            ("server", Side::Server),
            ("both-required", Side::BothRequired),
            ("both-optional", Side::BothOptional),
        ] {
            let parsed: ModToml = toml::from_str(&format!("side = \"{text}\"")).unwrap();
            assert_eq!(parsed.side, side);
        }
        assert!(toml::from_str::<ModToml>("side = \"both\"").is_err());
    }
}
