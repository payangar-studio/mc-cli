//! Formats of the repository and of the machine: the types every command
//! reads and writes, and nothing that talks to the network.

pub mod installed;
pub mod instance;
pub mod manifest;
pub mod socle;
pub mod variants;

use mc_core::model::{parse_toml, read_file, to_toml, write_file};
pub use mc_core::model::{sha256_bytes, sha256_file};
