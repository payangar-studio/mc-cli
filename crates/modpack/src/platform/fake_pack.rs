//! The modpack operations of the in-memory test platform: it writes a
//! plain-text "pack".

use std::fs;
use std::path::PathBuf;

use super::fake::FakePlatform;
use super::{Eligibility, PackBuild, PackPlatform};
use crate::error::Result;
use crate::model::manifest::Entry;

impl PackPlatform for FakePlatform {
    fn pack_eligibility(&self, entry: &Entry) -> Result<Eligibility> {
        Ok(match self.of_entry(entry) {
            Ok(_) => Eligibility::Eligible,
            Err(_) => Eligibility::Blocked {
                reason: format!("not hosted on {}", self.name),
            },
        })
    }

    /// Writes `<stem>.<name>`: one line per pack field, mod key and override.
    fn build_pack(&self, build: &PackBuild<'_>) -> Result<PathBuf> {
        let mut lines = vec![
            format!("name {}", build.name),
            format!("version {}", build.version),
            format!("minecraft {}", build.pack.minecraft),
        ];
        lines.extend(build.entries.iter().map(|e| format!("mod {}", e.key)));
        lines.extend(
            build
                .overrides
                .iter()
                .map(|o| format!("override {}", o.path)),
        );
        let path = build
            .out_dir
            .join(format!("{}.{}", build.file_stem, self.name));
        fs::write(&path, lines.join("\n") + "\n")?;
        Ok(path)
    }
}
