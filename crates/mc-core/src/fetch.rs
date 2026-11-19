//! Reading a URL: the seam between the code that downloads and the network.

use std::io::Read;

use crate::error::{Error, Result};

/// Opens a URL for reading.
pub trait Fetch: Sync {
    fn get(&self, url: &str) -> Result<Box<dyn Read + Send>>;
}

/// [`Fetch`] over HTTP.
pub struct Http {
    agent: ureq::Agent,
}

impl Http {
    pub fn new() -> Self {
        let agent = ureq::Agent::config_builder()
            .user_agent(concat!(
                "payangar-studio/modpack-cli/",
                env!("CARGO_PKG_VERSION")
            ))
            .build()
            .new_agent();
        Self { agent }
    }
}

impl Default for Http {
    fn default() -> Self {
        Self::new()
    }
}

impl Fetch for Http {
    fn get(&self, url: &str) -> Result<Box<dyn Read + Send>> {
        let response = self.agent.get(url).call().map_err(|e| {
            Error::internal(
                format!("cannot download {url}"),
                "check the network connection; if the URL is gone, regenerate the manifest with `modpack manifest`",
            )
            .with_source(e)
        })?;
        Ok(Box::new(response.into_body().into_reader()))
    }
}

/// Test doubles of [`Fetch`].
#[cfg(any(test, feature = "fake"))]
pub mod fake {
    use std::collections::BTreeMap;
    use std::io::{Cursor, Read};

    use super::Fetch;
    use crate::error::{Error, Result};

    /// Serves fixed bytes per URL; any other URL fails.
    #[derive(Default)]
    pub struct MapFetch(pub BTreeMap<String, Vec<u8>>);

    impl MapFetch {
        pub fn with(mut self, url: &str, bytes: &[u8]) -> Self {
            self.0.insert(url.to_owned(), bytes.to_vec());
            self
        }
    }

    impl Fetch for MapFetch {
        fn get(&self, url: &str) -> Result<Box<dyn Read + Send>> {
            match self.0.get(url) {
                Some(bytes) => Ok(Box::new(Cursor::new(bytes.clone()))),
                None => Err(Error::internal(format!("404 for {url}"), "none")),
            }
        }
    }

    /// Fails the test if anything is fetched.
    pub struct NoFetch;

    impl Fetch for NoFetch {
        fn get(&self, url: &str) -> Result<Box<dyn Read + Send>> {
            panic!("nothing should be fetched, got {url}")
        }
    }
}
