//! BUD-10 Blossom URI parsing and construction.

use std::fmt;
use std::str::FromStr;

use bitcoin_hashes::sha256::Hash as Sha256Hash;
use nostr::key::PublicKey;
use nostr::types::Url;

use crate::error::{Error, ErrorKind};

/// A content-addressed Blossom URI with ordered discovery hints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlossomUri {
    /// SHA256 content address.
    pub sha256: Sha256Hash,
    /// Required file extension, without the leading dot.
    pub extension: String,
    /// Ordered server hints from repeated `xs` parameters.
    pub servers: Vec<String>,
    /// Ordered author hints from repeated `as` parameters.
    pub authors: Vec<PublicKey>,
    /// Expected blob size in bytes.
    pub size: Option<u64>,
}

impl BlossomUri {
    /// Constructs a minimal URI, using `bin` when the extension is unknown.
    pub fn new<S>(sha256: Sha256Hash, extension: S) -> Self
    where
        S: Into<String>,
    {
        let extension = extension.into();
        Self {
            sha256,
            extension: if extension.is_empty() {
                "bin".to_owned()
            } else {
                extension
            },
            servers: Vec::new(),
            authors: Vec::new(),
            size: None,
        }
    }

    /// Builds ordered candidate URLs from URI hints and caller-resolved server lists.
    ///
    /// Bare server hints produce HTTPS followed by HTTP, as required by BUD-10.
    pub fn candidate_urls(
        &self,
        author_servers: impl IntoIterator<Item = Url>,
        fallback_servers: impl IntoIterator<Item = Url>,
    ) -> Vec<Url> {
        let path = format!("{}.{}", self.sha256, self.extension);
        let mut candidates = Vec::new();
        for server in &self.servers {
            if server.contains("://") {
                if let Ok(mut url) = Url::parse(server) {
                    url.set_path(&path);
                    candidates.push(url);
                }
            } else {
                for scheme in ["https", "http"] {
                    if let Ok(url) = Url::parse(&format!("{scheme}://{server}/{path}")) {
                        candidates.push(url);
                    }
                }
            }
        }
        for mut server in author_servers.into_iter().chain(fallback_servers) {
            server.set_path(&path);
            server.set_query(None);
            server.set_fragment(None);
            candidates.push(server);
        }
        candidates.dedup();
        candidates
    }
}

impl FromStr for BlossomUri {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let body = value.strip_prefix("blossom:").ok_or_else(|| {
            Error::with_static_message(ErrorKind::Malformed, "Missing blossom URI scheme")
        })?;
        let (path, query) = body.split_once('?').unwrap_or((body, ""));
        let (hash, extension) = path.split_once('.').ok_or_else(|| {
            Error::with_static_message(ErrorKind::Malformed, "Missing blossom URI extension")
        })?;
        if extension.is_empty() {
            return Err(Error::with_static_message(
                ErrorKind::Malformed,
                "Empty blossom URI extension",
            ));
        }
        let sha256 = hash.parse().map_err(|_| {
            Error::with_static_message(ErrorKind::Malformed, "Invalid blossom URI SHA256")
        })?;
        let query_url = Url::parse(&format!("https://blossom.invalid/?{query}"))?;
        let mut servers = Vec::new();
        let mut authors = Vec::new();
        let mut size = None;
        for (key, value) in query_url.query_pairs() {
            match key.as_ref() {
                "xs" => servers.push(value.into_owned()),
                "as" => authors.push(PublicKey::parse(value.as_ref())?),
                "sz" => {
                    let parsed = value.parse::<u64>().map_err(|_| {
                        Error::with_static_message(ErrorKind::Malformed, "Invalid blossom URI size")
                    })?;
                    if parsed == 0 {
                        return Err(Error::with_static_message(
                            ErrorKind::Malformed,
                            "Blossom URI size must be positive",
                        ));
                    }
                    size = Some(parsed);
                }
                _ => {}
            }
        }
        Ok(Self {
            sha256,
            extension: extension.to_owned(),
            servers,
            authors,
            size,
        })
    }
}

impl fmt::Display for BlossomUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "blossom:{}.{}", self.sha256, self.extension)?;
        let mut query_url = Url::parse("https://blossom.invalid/").map_err(|_| fmt::Error)?;
        {
            let mut query = query_url.query_pairs_mut();
            for server in &self.servers {
                query.append_pair("xs", server);
            }
            for author in &self.authors {
                query.append_pair("as", &author.to_hex());
            }
            if let Some(size) = self.size {
                query.append_pair("sz", &size.to_string());
            }
        }
        if let Some(query) = query_url.query() {
            write!(f, "?{query}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "b1674191a88ec5cdd733e4240a81803105dc412d6c6708d53ab94fc248f4f553";

    #[test]
    fn roundtrips_uri_and_preserves_hint_order() {
        let value = format!(
            "blossom:{HASH}.pdf?xs=cdn.example.com&xs=https%3A%2F%2Fother.example&sz=184292"
        );
        let uri: BlossomUri = value.parse().unwrap();

        assert_eq!(uri.size, Some(184292));
        assert_eq!(uri.servers, ["cdn.example.com", "https://other.example"]);
        assert_eq!(uri.to_string(), value);
    }

    #[test]
    fn bare_server_candidates_prefer_https() {
        let uri: BlossomUri = format!("blossom:{HASH}.bin?xs=cdn.example.com")
            .parse()
            .unwrap();
        let candidates = uri.candidate_urls([], []);

        assert_eq!(candidates[0].scheme(), "https");
        assert_eq!(candidates[1].scheme(), "http");
    }
}
