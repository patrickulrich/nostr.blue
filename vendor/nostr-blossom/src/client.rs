//! Implements a Blossom client for interacting with Blossom servers

use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use bitcoin_hashes::sha256::Hash as Sha256Hash;
use nostr::prelude::*;
use nostr::types::Url;
#[cfg(not(target_arch = "wasm32"))]
use reqwest::header::LOCATION;
use reqwest::header::{AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, HeaderMap, HeaderValue, RANGE};
#[cfg(not(target_arch = "wasm32"))]
use reqwest::redirect::Policy;
use reqwest::{Response, StatusCode};
use serde::Serialize;

use crate::bud01::{BlossomAuthorization, BlossomAuthorizationScope, BlossomAuthorizationVerb};
use crate::bud02::BlobDescriptor;
use crate::bud10::BlossomUri;
use crate::error::{Error, ErrorKind};

// nostr.blue: hard ceiling for any single blob fetch (BUD-10 resolution and
// GET /<sha256>). Guards against servers that omit or lie about
// Content-Length; the streaming reader below also aborts the moment this cap
// (or a tighter expected-size cap) is exceeded.
const BLOB_FETCH_HARD_CAP: u64 = 64 * 1024 * 1024;

// nostr.blue: per-request timeout for blob fetches. Set per request (not on
// the client builder) because reqwest's wasm backend has no builder-level
// timeout; `RequestBuilder::timeout` works on every backend and on native
// covers the body read as well as the headers.
const BLOB_FETCH_TIMEOUT: Duration = Duration::from_secs(30);

// nostr.blue: effective byte cap for one fetch — the expected size when one
// is known (BUD-10 `sz`), clamped to the hard cap; the hard cap otherwise.
fn effective_fetch_cap(expected_size: Option<u64>, hard_cap: u64) -> u64 {
    expected_size.map_or(hard_cap, |size| size.min(hard_cap))
}

// nostr.blue: running-cap decision — would appending `chunk_len` bytes to an
// already-`received` buffer exceed `cap`? Pure so the abort semantics are
// unit-testable without a network.
fn chunk_exceeds_cap(received: usize, chunk_len: usize, cap: u64) -> bool {
    received.saturating_add(chunk_len) as u64 > cap
}

// nostr.blue: reported Content-Length, when the server sends a readable one
// (it is a CORS-safelisted response header, so this works cross-origin too).
fn response_content_length(response: &Response) -> Option<u64> {
    response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
}

/// A client for interacting with a Blossom server
///
/// <https://github.com/hzrd149/blossom>
#[derive(Debug, Clone)]
pub struct BlossomClient {
    base_url: Url,
    client: reqwest::Client,
}

impl BlossomClient {
    /// Creates a new `BlossomClient` with the given base URL.
    pub fn new(mut base_url: Url) -> Self {
        base_url.set_path("/");
        base_url.set_query(None);
        base_url.set_fragment(None);
        Self {
            base_url,
            client: Self::build_client().unwrap(),
        }
    }

    /// Builds the reqwest client
    fn build_client() -> reqwest::Result<reqwest::Client> {
        let builder = reqwest::Client::builder();
        #[cfg(not(target_arch = "wasm32"))]
        let builder = builder.redirect(Policy::none());
        builder.build()
    }

    /// Uploads a blob to the Blossom server.
    ///
    /// <https://github.com/hzrd149/blossom/blob/master/buds/12.md>
    pub async fn upload_blob<T>(
        &self,
        data: Vec<u8>,
        content_type: Option<String>,
        authorization_options: Option<BlossomAuthorizationOptions>,
        signer: Option<&T>,
    ) -> Result<BlobDescriptor, Error>
    where
        T: AsyncGetPublicKey + AsyncSignEvent,
    {
        self.upload_blob_with_payment(data, content_type, authorization_options, signer, None)
            .await
    }

    /// Uploads a blob with an optional BUD-07 payment proof.
    pub async fn upload_blob_with_payment<T>(
        &self,
        data: Vec<u8>,
        content_type: Option<String>,
        authorization_options: Option<BlossomAuthorizationOptions>,
        signer: Option<&T>,
        payment: Option<&BlossomPaymentProof>,
    ) -> Result<BlobDescriptor, Error>
    where
        T: AsyncGetPublicKey + AsyncSignEvent,
    {
        let url: Url = self.base_url.join("upload")?;

        let hash: Sha256Hash = Sha256Hash::hash(&data);
        let file_hashes: Vec<Sha256Hash> = vec![hash];

        let data_len = data.len();
        let mut request = self.client.put(url).body(data);
        let mut headers = HeaderMap::new();

        headers.insert(CONTENT_LENGTH, HeaderValue::from(data_len));
        headers.insert("X-SHA-256", HeaderValue::from_str(&hash.to_string())?);

        if let Some(ct) = content_type {
            headers.insert(CONTENT_TYPE, HeaderValue::from_str(&ct)?);
        }
        Self::add_payment_header(&mut headers, payment)?;

        if let Some(signer) = signer {
            let default_auth = self.default_auth(
                BlossomAuthorizationVerb::Upload,
                "Blossom upload authorization",
                BlossomAuthorizationScope::BlobSha256Hashes(file_hashes),
            );
            let final_auth = authorization_options
                .map(|opts| Self::update_authorization_fixture(&default_auth, opts))
                .unwrap_or(default_auth);
            let auth_header = Self::build_auth_header(signer, final_auth).await?;
            headers.insert(AUTHORIZATION, auth_header);
        }

        request = request.headers(headers);

        let response: Response = request.send().await?;

        match response.status() {
            StatusCode::OK | StatusCode::CREATED => {
                let descriptor: BlobDescriptor = response.json().await?;
                // nostr.blue: BUD-02 — the server MUST NOT modify the blob on
                // PUT /upload. A descriptor whose hash differs from the
                // locally computed one describes different bytes (buggy or
                // hostile server), so reject it instead of trusting it.
                if descriptor.sha256 != hash {
                    return Err(Error::new(
                        ErrorKind::Invalid,
                        format!(
                            "BUD-02 violation: server stored different bytes \
                             (expected sha256 {hash}, descriptor says {})",
                            descriptor.sha256
                        ),
                    ));
                }
                // nostr.blue: the descriptor URL must be absolute http(s) and
                // served from the upload server's own origin — rejects
                // javascript:/data:/attacker-origin descriptors.
                Self::validate_descriptor_url(&descriptor.url, &self.base_url)?;
                Ok(descriptor)
            }
            _ => Err(Error::response("Failed to upload blob", response)),
        }
    }

    /// Checks whether the server would accept a blob upload without sending its body.
    ///
    /// This implements the optional BUD-06 `HEAD /upload` preflight endpoint.
    pub async fn upload_requirements<T>(
        &self,
        sha256: Sha256Hash,
        size: u64,
        content_type: Option<&str>,
        authorization_options: Option<BlossomAuthorizationOptions>,
        signer: Option<&T>,
    ) -> Result<BlossomPreflight, Error>
    where
        T: AsyncGetPublicKey + AsyncSignEvent,
    {
        self.preflight(
            "upload",
            sha256,
            size,
            content_type,
            BlossomAuthorizationVerb::Upload,
            authorization_options,
            signer,
        )
        .await
    }

    /// Lists blobs uploaded by a specific pubkey.
    ///
    /// <https://github.com/hzrd149/blossom/blob/master/buds/02.md>
    pub async fn list_blobs<T>(
        &self,
        pubkey: &PublicKey,
        since: Option<Timestamp>,
        until: Option<Timestamp>,
        authorization_options: Option<BlossomAuthorizationOptions>,
        signer: Option<&T>,
    ) -> Result<Vec<BlobDescriptor>, Error>
    where
        T: AsyncGetPublicKey + AsyncSignEvent,
    {
        let mut url: Url = self.base_url.join(&format!("list/{}", pubkey.to_hex()))?;

        if let Some(since) = since {
            url.query_pairs_mut()
                .append_pair("since", since.to_string().as_str());
        }

        if let Some(until) = until {
            url.query_pairs_mut()
                .append_pair("until", until.to_string().as_str());
        }

        let mut request = self.client.get(url);
        let mut headers = HeaderMap::new();

        if let Some(signer) = signer {
            let default_auth = self.default_auth(
                BlossomAuthorizationVerb::List,
                "Blossom list authorization",
                BlossomAuthorizationScope::ServerUrl(self.base_url.clone()),
            );
            let final_auth = authorization_options
                .map(|opts| Self::update_authorization_fixture(&default_auth, opts))
                .unwrap_or(default_auth);
            let auth_header = Self::build_auth_header(signer, final_auth).await?;
            headers.insert(AUTHORIZATION, auth_header);
        }

        request = request.headers(headers);

        let response: Response = request.send().await?;

        match response.status() {
            StatusCode::OK => {
                let descriptors: Vec<BlobDescriptor> = response.json().await?;
                Ok(descriptors)
            }
            _ => Err(Error::response("Failed to list blobs", response)),
        }
    }

    /// Lists one cursor-paginated page of blobs according to BUD-12.
    pub async fn list_blobs_page<T>(
        &self,
        pubkey: &PublicKey,
        options: BlossomListOptions,
        authorization_options: Option<BlossomAuthorizationOptions>,
        signer: Option<&T>,
        payment: Option<&BlossomPaymentProof>,
    ) -> Result<Vec<BlobDescriptor>, Error>
    where
        T: AsyncGetPublicKey + AsyncSignEvent,
    {
        let mut url = self.base_url.join(&format!("list/{}", pubkey.to_hex()))?;
        {
            let mut query = url.query_pairs_mut();
            if let Some(cursor) = options.cursor {
                query.append_pair("cursor", &cursor.to_string());
            }
            if let Some(limit) = options.limit {
                query.append_pair("limit", &limit.to_string());
            }
            if let Some(since) = options.since {
                query.append_pair("since", &since.to_string());
            }
            if let Some(until) = options.until {
                query.append_pair("until", &until.to_string());
            }
        }

        let mut headers = HeaderMap::new();
        Self::add_payment_header(&mut headers, payment)?;
        self.add_authorization_header(
            &mut headers,
            BlossomAuthorizationVerb::List,
            "Blossom list authorization",
            BlossomAuthorizationScope::ServerUrl(self.base_url.clone()),
            authorization_options,
            signer,
        )
        .await?;
        let response = self.client.get(url).headers(headers).send().await?;
        if response.status() == StatusCode::OK {
            Ok(response.json().await?)
        } else {
            Err(Error::response("Failed to list blobs", response))
        }
    }

    /// Retrieves a blob from the Blossom server, with optional authorization.
    ///
    /// <https://github.com/hzrd149/blossom/blob/master/buds/01.md>
    pub async fn get_blob<T>(
        &self,
        sha256: Sha256Hash,
        range: Option<String>,
        authorization_options: Option<BlossomAuthorizationOptions>,
        signer: Option<&T>,
    ) -> Result<Vec<u8>, Error>
    where
        T: AsyncGetPublicKey + AsyncSignEvent,
    {
        self.get_blob_with_payment(sha256, range, authorization_options, signer, None)
            .await
    }

    /// Retrieves a blob with an optional BUD-07 payment proof.
    pub async fn get_blob_with_payment<T>(
        &self,
        sha256: Sha256Hash,
        range: Option<String>,
        authorization_options: Option<BlossomAuthorizationOptions>,
        signer: Option<&T>,
        payment: Option<&BlossomPaymentProof>,
    ) -> Result<Vec<u8>, Error>
    where
        T: AsyncGetPublicKey + AsyncSignEvent,
    {
        let url: Url = self.base_url.join(sha256.to_string().as_str())?;

        // nostr.blue: bound every fetch (per-request timeout + capped body —
        // see BLOB_FETCH_TIMEOUT / BLOB_FETCH_HARD_CAP).
        let mut request = self.client.get(url).timeout(BLOB_FETCH_TIMEOUT);
        let mut headers = HeaderMap::new();

        let verify_hash = range.is_none();
        if let Some(range_value) = range {
            headers.insert(RANGE, HeaderValue::from_str(&range_value)?);
        }
        Self::add_payment_header(&mut headers, payment)?;

        if let Some(signer) = signer {
            let default_auth = self.default_auth(
                BlossomAuthorizationVerb::Get,
                "Blossom get authorization",
                BlossomAuthorizationScope::BlobSha256Hashes(vec![sha256]),
            );
            let final_auth = authorization_options
                .map(|opts| Self::update_authorization_fixture(&default_auth, opts))
                .unwrap_or(default_auth);
            let auth_header = Self::build_auth_header(signer, final_auth).await?;
            headers.insert(AUTHORIZATION, auth_header);
        }

        request = request.headers(headers.clone());

        #[cfg_attr(target_arch = "wasm32", allow(unused_mut))]
        let mut response: Response = request.send().await?;

        #[cfg(not(target_arch = "wasm32"))]
        for _ in 0..10 {
            if !response.status().is_redirection() {
                break;
            }

            let location = response.headers().get(LOCATION).ok_or_else(|| {
                Error::with_static_message(
                    ErrorKind::Invalid,
                    "Redirect response missing 'Location' header",
                )
            })?;
            let next_url = response.url().join(location.to_str()?)?;
            if !next_url.as_str().contains(&sha256.to_string()) {
                return Err(Error::with_static_message(
                    ErrorKind::Invalid,
                    "Redirect URL does not contain SHA256",
                ));
            }

            let same_origin = next_url.origin() == response.url().origin();
            let mut next_headers = headers.clone();
            if !same_origin {
                next_headers.remove(AUTHORIZATION);
            }
            response = self
                .client
                .get(next_url)
                .timeout(BLOB_FETCH_TIMEOUT)
                .headers(next_headers)
                .send()
                .await?;
        }

        if response.status().is_redirection() {
            return Err(Error::with_static_message(
                ErrorKind::Invalid,
                "Too many blob redirects",
            ));
        }

        match response.status() {
            StatusCode::OK | StatusCode::PARTIAL_CONTENT => {
                // nostr.blue: bound the download. `response.bytes()` used to
                // buffer unbounded bodies; now the advertised Content-Length
                // is checked before reading and the streamed read aborts the
                // moment the hard cap is exceeded.
                if response_content_length(&response).is_some_and(|len| len > BLOB_FETCH_HARD_CAP) {
                    return Err(Error::new(
                        ErrorKind::Invalid,
                        format!("Blob exceeds the {}-byte download cap", BLOB_FETCH_HARD_CAP),
                    ));
                }
                let data = Self::read_body_capped(response, BLOB_FETCH_HARD_CAP).await?;
                if verify_hash && Sha256Hash::hash(&data) != sha256 {
                    return Err(Error::with_static_message(
                        ErrorKind::Invalid,
                        "Downloaded blob does not match requested SHA256",
                    ));
                }
                Ok(data)
            }
            _ => Err(Error::response("Failed to get blob", response)),
        }
    }

    /// Checks if a blob exists on the Blossom server.
    ///
    /// <https://github.com/hzrd149/blossom/blob/master/buds/01.md>
    pub async fn has_blob<T>(
        &self,
        sha256: Sha256Hash,
        authorization_options: Option<BlossomAuthorizationOptions>,
        signer: Option<&T>,
    ) -> Result<bool, Error>
    where
        T: AsyncGetPublicKey + AsyncSignEvent,
    {
        // nostr.blue: `Some` from `head_blob` = server answered 200 OK.
        Ok(self
            .head_blob(sha256, authorization_options, signer)
            .await?
            .is_some())
    }

    /// Checks if a blob exists and reports the server's `Content-Length`.
    ///
    /// # nostr.blue
    /// A bare HEAD 200 is not proof a blob exists — SPA-fallback origins
    /// answer 200 to every HEAD, which used to cancel uploads silently.
    /// Returns `Ok(Some(len))` for a 200 with a readable Content-Length,
    /// `Ok(None)` when the blob is absent (404) or present without a
    /// readable size, and `Err` for anything inconclusive (5xx, network).
    /// Callers should only trust a dedupe shortcut when `len` matches the
    /// size of the bytes they are holding.
    pub async fn has_blob_with_size<T>(
        &self,
        sha256: Sha256Hash,
        authorization_options: Option<BlossomAuthorizationOptions>,
        signer: Option<&T>,
    ) -> Result<Option<u64>, Error>
    where
        T: AsyncGetPublicKey + AsyncSignEvent,
    {
        Ok(self
            .head_blob(sha256, authorization_options, signer)
            .await?
            .flatten())
    }

    // nostr.blue: shared HEAD /<sha256> probe backing `has_blob` and
    // `has_blob_with_size`. Ok(Some(None)) = 200 without a readable
    // Content-Length, Ok(None) = 404.
    async fn head_blob<T>(
        &self,
        sha256: Sha256Hash,
        authorization_options: Option<BlossomAuthorizationOptions>,
        signer: Option<&T>,
    ) -> Result<Option<Option<u64>>, Error>
    where
        T: AsyncGetPublicKey + AsyncSignEvent,
    {
        let url: Url = self.base_url.join(sha256.to_string().as_str())?;

        let mut request = self.client.head(url);

        if let Some(signer) = signer {
            let default_auth = self.default_auth(
                BlossomAuthorizationVerb::Get,
                "Blossom get authorization",
                BlossomAuthorizationScope::BlobSha256Hashes(vec![sha256]),
            );

            let final_auth = authorization_options
                .map(|opts| Self::update_authorization_fixture(&default_auth, opts))
                .unwrap_or(default_auth);

            let mut headers = HeaderMap::new();
            let auth_header = Self::build_auth_header(signer, final_auth).await?;
            headers.insert(AUTHORIZATION, auth_header);

            request = request.headers(headers);
        }

        let response: Response = request.send().await?;

        match response.status() {
            StatusCode::OK => Ok(Some(response_content_length(&response))),
            StatusCode::NOT_FOUND => Ok(None),
            _ => Err(Error::response("Unexpected HTTP status code", response)),
        }
    }

    /// Deletes a blob from the Blossom server.
    ///
    /// <https://github.com/hzrd149/blossom/blob/master/buds/02.md>
    pub async fn delete_blob<T>(
        &self,
        sha256: Sha256Hash,
        authorization_options: Option<BlossomAuthorizationOptions>,
        signer: &T,
    ) -> Result<(), Error>
    where
        T: AsyncGetPublicKey + AsyncSignEvent,
    {
        self.delete_blob_with_payment(sha256, authorization_options, signer, None)
            .await
    }

    /// Deletes a blob with an optional BUD-07 payment proof.
    pub async fn delete_blob_with_payment<T>(
        &self,
        sha256: Sha256Hash,
        authorization_options: Option<BlossomAuthorizationOptions>,
        signer: &T,
        payment: Option<&BlossomPaymentProof>,
    ) -> Result<(), Error>
    where
        T: AsyncGetPublicKey + AsyncSignEvent,
    {
        let url: Url = self.base_url.join(sha256.to_string().as_str())?;

        let mut headers = HeaderMap::new();
        let default_auth = self.default_auth(
            BlossomAuthorizationVerb::Delete,
            "Blossom delete authorization",
            BlossomAuthorizationScope::BlobSha256Hashes(vec![sha256]),
        );

        let final_auth = authorization_options
            .map(|opts| Self::update_authorization_fixture(&default_auth, opts))
            .unwrap_or(default_auth);

        let auth_header = Self::build_auth_header(signer, final_auth).await?;
        headers.insert(AUTHORIZATION, auth_header);
        Self::add_payment_header(&mut headers, payment)?;

        let response: Response = self.client.delete(url).headers(headers).send().await?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(Error::response("Failed to delete blob", response))
        }
    }

    /// Mirrors an existing blob from its public URL.
    ///
    /// This implements BUD-04. The authorization uses the `upload` verb and the
    /// mirrored blob's hash as required by BUD-11.
    pub async fn mirror_blob<T>(
        &self,
        blob: &BlobDescriptor,
        authorization_options: Option<BlossomAuthorizationOptions>,
        signer: Option<&T>,
        payment: Option<&BlossomPaymentProof>,
    ) -> Result<BlobDescriptor, Error>
    where
        T: AsyncGetPublicKey + AsyncSignEvent,
    {
        #[derive(Serialize)]
        struct MirrorRequest<'a> {
            url: &'a Url,
        }

        let url = self.base_url.join("mirror")?;
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            "X-SHA-256",
            HeaderValue::from_str(&blob.sha256.to_string())?,
        );
        headers.insert("X-Content-Length", HeaderValue::from(blob.size));
        headers.insert("X-Content-Type", HeaderValue::from_str(&blob.mime_type)?);
        Self::add_payment_header(&mut headers, payment)?;
        self.add_authorization_header(
            &mut headers,
            BlossomAuthorizationVerb::Upload,
            "Blossom mirror authorization",
            BlossomAuthorizationScope::BlobSha256Hashes(vec![blob.sha256]),
            authorization_options,
            signer,
        )
        .await?;

        let response = self
            .client
            .put(url)
            .headers(headers)
            .json(&MirrorRequest { url: &blob.url })
            .send()
            .await?;
        Self::descriptor_response("Failed to mirror blob", response).await
    }

    /// Uploads to the first BUD-03 server and mirrors to every remaining server.
    ///
    /// Descriptors are returned in server-list order. Processing stops on the
    /// first failed upload or mirror request.
    pub async fn upload_and_mirror<T>(
        servers: &[Url],
        data: Vec<u8>,
        content_type: Option<String>,
        authorization_options: Option<BlossomAuthorizationOptions>,
        signer: Option<&T>,
    ) -> Result<Vec<BlobDescriptor>, Error>
    where
        T: AsyncGetPublicKey + AsyncSignEvent,
    {
        let (first, mirrors) = servers.split_first().ok_or_else(|| {
            Error::with_static_message(ErrorKind::Invalid, "A Blossom server list cannot be empty")
        })?;
        let descriptor = Self::new(first.clone())
            .upload_blob(data, content_type, authorization_options.clone(), signer)
            .await?;
        let mut descriptors = Vec::with_capacity(servers.len());
        descriptors.push(descriptor.clone());
        for server in mirrors {
            let mirrored = Self::new(server.clone())
                .mirror_blob(&descriptor, authorization_options.clone(), signer, None)
                .await?;
            descriptors.push(mirrored);
        }
        Ok(descriptors)
    }

    /// Checks whether the server would accept media for optimization.
    pub async fn media_requirements<T>(
        &self,
        sha256: Sha256Hash,
        size: u64,
        content_type: Option<&str>,
        authorization_options: Option<BlossomAuthorizationOptions>,
        signer: Option<&T>,
    ) -> Result<BlossomPreflight, Error>
    where
        T: AsyncGetPublicKey + AsyncSignEvent,
    {
        self.preflight(
            "media",
            sha256,
            size,
            content_type,
            BlossomAuthorizationVerb::Media,
            authorization_options,
            signer,
        )
        .await
    }

    /// Uploads media for server-selected optimization according to BUD-05.
    pub async fn upload_media<T>(
        &self,
        data: Vec<u8>,
        content_type: Option<String>,
        authorization_options: Option<BlossomAuthorizationOptions>,
        signer: Option<&T>,
        payment: Option<&BlossomPaymentProof>,
    ) -> Result<BlobDescriptor, Error>
    where
        T: AsyncGetPublicKey + AsyncSignEvent,
    {
        let url = self.base_url.join("media")?;
        let sha256 = Sha256Hash::hash(&data);
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_LENGTH, HeaderValue::from(data.len()));
        headers.insert("X-SHA-256", HeaderValue::from_str(&sha256.to_string())?);
        if let Some(content_type) = content_type {
            headers.insert(CONTENT_TYPE, HeaderValue::from_str(&content_type)?);
        }
        Self::add_payment_header(&mut headers, payment)?;
        self.add_authorization_header(
            &mut headers,
            BlossomAuthorizationVerb::Media,
            "Blossom media authorization",
            BlossomAuthorizationScope::BlobSha256Hashes(vec![sha256]),
            authorization_options,
            signer,
        )
        .await?;

        let response = self
            .client
            .put(url)
            .headers(headers)
            .body(data)
            .send()
            .await?;
        let descriptor = Self::descriptor_response("Failed to optimize media", response).await?;
        // nostr.blue: BUD-05 allows the server to transform the blob, so the
        // descriptor hash/size legitimately describe the *optimized* bytes
        // and are not compared here. The URL, however, must still be absolute
        // http(s) and served by this server — the same origin-equality rule
        // nostr.blue applies to list/mirror responses (rejects
        // javascript:/data:/attacker-origin descriptors).
        Self::validate_descriptor_url(&descriptor.url, &self.base_url)?;
        Ok(descriptor)
    }

    /// Submits a signed NIP-56 blob report according to BUD-09.
    pub async fn report_blobs(&self, report: &Event) -> Result<(), Error> {
        let has_blob_hash = report.tags.iter().any(|tag| {
            let values = tag.as_slice();
            values.first().map(String::as_str) == Some("x") && values.get(1).is_some()
        });
        if report.kind != Kind::Reporting || !has_blob_hash {
            return Err(Error::with_static_message(
                ErrorKind::Invalid,
                "Blob reports must be kind 1984 and contain an x tag",
            ));
        }

        let response = self
            .client
            .put(self.base_url.join("report")?)
            .header(CONTENT_TYPE, "application/json")
            .body(report.as_json())
            .send()
            .await?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(Error::response("Failed to report blob", response))
        }
    }

    /// Resolves a BUD-10 URI using ordered URI, author-list, and fallback servers.
    pub async fn resolve_uri(
        &self,
        uri: &BlossomUri,
        author_servers: impl IntoIterator<Item = Url>,
        fallback_servers: impl IntoIterator<Item = Url>,
    ) -> Result<Vec<u8>, Error> {
        // nostr.blue: bounded resolution. This used to build a bare
        // `reqwest::Client::new()` per call (no timeout, default redirect
        // policy) and buffer whole bodies via `response.bytes()` before any
        // size check. It now shares `self.client` (redirects refused on
        // native, matching `get_blob`; wasm was already following them),
        // applies a per-request timeout, skips candidates whose advertised
        // Content-Length already exceeds the cap, and aborts the streamed
        // read the moment the cap is exceeded.
        let cap = effective_fetch_cap(uri.size, BLOB_FETCH_HARD_CAP);
        for candidate in uri.candidate_urls(author_servers, fallback_servers) {
            let Ok(response) = self
                .client
                .get(candidate)
                .timeout(BLOB_FETCH_TIMEOUT)
                .send()
                .await
            else {
                continue;
            };
            if !response.status().is_success()
                || !response.url().as_str().contains(&uri.sha256.to_string())
            {
                continue;
            }
            if let Some(len) = response_content_length(&response) {
                if len > cap {
                    continue;
                }
            }
            let Ok(data) = Self::read_body_capped(response, cap).await else {
                continue;
            };
            if uri.size.is_none_or(|size| size == data.len() as u64)
                && Sha256Hash::hash(&data) == uri.sha256
            {
                return Ok(data);
            }
        }
        Err(Error::with_static_message(
            ErrorKind::Invalid,
            "Blob could not be resolved from any server",
        ))
    }

    // nostr.blue: upstream signature — the preflight endpoint mirrors
    // BUD-06's parameter surface; kept as-is despite the arg count.
    #[allow(clippy::too_many_arguments)]
    async fn preflight<T>(
        &self,
        endpoint: &str,
        sha256: Sha256Hash,
        size: u64,
        content_type: Option<&str>,
        verb: BlossomAuthorizationVerb,
        authorization_options: Option<BlossomAuthorizationOptions>,
        signer: Option<&T>,
    ) -> Result<BlossomPreflight, Error>
    where
        T: AsyncGetPublicKey + AsyncSignEvent,
    {
        let url = self.base_url.join(endpoint)?;
        let mut headers = HeaderMap::new();
        headers.insert("X-SHA-256", HeaderValue::from_str(&sha256.to_string())?);
        headers.insert("X-Content-Length", HeaderValue::from(size));
        if let Some(content_type) = content_type {
            headers.insert("X-Content-Type", HeaderValue::from_str(content_type)?);
        }
        self.add_authorization_header(
            &mut headers,
            verb,
            "Blossom upload preflight authorization",
            BlossomAuthorizationScope::BlobSha256Hashes(vec![sha256]),
            authorization_options,
            signer,
        )
        .await?;

        let response = self.client.head(url).headers(headers).send().await?;
        match response.status() {
            StatusCode::OK => Ok(BlossomPreflight::Accepted),
            StatusCode::PAYMENT_REQUIRED => {
                // nostr.blue: whitelist the BUD-07 payment methods (see
                // `parse_payment_request_headers`) — unrelated `X-*` headers
                // (X-Content-Type-Options, X-RateLimit-*, …) must not be
                // misread as payment requests.
                let (requests, _reason) = parse_payment_request_headers(response.headers());
                if requests.is_empty() || requests.iter().any(|request| !request.is_valid()) {
                    return Err(Error::with_static_message(
                        ErrorKind::Malformed,
                        "Invalid BUD-07 payment challenge",
                    ));
                }
                Ok(BlossomPreflight::PaymentRequired(requests))
            }
            StatusCode::NOT_FOUND => Ok(BlossomPreflight::Unsupported),
            _ => Err(Error::response("Upload preflight failed", response)),
        }
    }

    async fn add_authorization_header<T>(
        &self,
        headers: &mut HeaderMap,
        verb: BlossomAuthorizationVerb,
        content: &'static str,
        scope: BlossomAuthorizationScope,
        authorization_options: Option<BlossomAuthorizationOptions>,
        signer: Option<&T>,
    ) -> Result<(), Error>
    where
        T: AsyncGetPublicKey + AsyncSignEvent,
    {
        if let Some(signer) = signer {
            let default_auth = self.default_auth(verb, content, scope);
            let authorization = authorization_options
                .map(|options| Self::update_authorization_fixture(&default_auth, options))
                .unwrap_or(default_auth);
            headers.insert(
                AUTHORIZATION,
                Self::build_auth_header(signer, authorization).await?,
            );
        }
        Ok(())
    }

    fn add_payment_header(
        headers: &mut HeaderMap,
        payment: Option<&BlossomPaymentProof>,
    ) -> Result<(), Error> {
        if let Some(payment) = payment {
            if !payment.is_valid() {
                return Err(Error::with_static_message(
                    ErrorKind::Malformed,
                    "Invalid BUD-07 payment proof",
                ));
            }
            let name = reqwest::header::HeaderName::from_bytes(
                format!("X-{}", payment.method).as_bytes(),
            )?;
            headers.insert(name, HeaderValue::from_str(&payment.proof)?);
        }
        Ok(())
    }

    async fn descriptor_response(
        error_message: &'static str,
        response: Response,
    ) -> Result<BlobDescriptor, Error> {
        match response.status() {
            StatusCode::OK | StatusCode::CREATED => Ok(response.json().await?),
            _ => Err(Error::response(error_message, response)),
        }
    }

    // nostr.blue: BUD-02/BUD-05 hardening — a server-returned descriptor URL
    // must be absolute http(s) and served from the same origin as the
    // endpoint the upload/optimization was sent to. Distinct error messages
    // for the scheme and origin rejections. Same-origin follows the
    // origin-equality rule nostr.blue already applies to list/mirror
    // responses (no CDN exceptions in-repo).
    fn validate_descriptor_url(descriptor_url: &Url, server: &Url) -> Result<(), Error> {
        let scheme = descriptor_url.scheme();
        if scheme != "http" && scheme != "https" {
            return Err(Error::new(
                ErrorKind::Invalid,
                format!(
                    "Blob descriptor URL must be absolute http(s), got {scheme:?}: {descriptor_url}"
                ),
            ));
        }
        if descriptor_url.origin() != server.origin() {
            return Err(Error::new(
                ErrorKind::Invalid,
                format!(
                    "Blob descriptor URL origin does not match the upload server \
                     ({descriptor_url} vs {server})"
                ),
            ));
        }
        Ok(())
    }

    // nostr.blue: accumulate a response body while enforcing a hard byte cap,
    // aborting mid-stream the moment the cap would be exceeded instead of
    // waiting for EOF. The per-chunk decision lives in `chunk_exceeds_cap`
    // so the abort semantics are unit-testable without a network.
    // (`mut` is only needed by the native `chunk()` loop — the wasm
    // `bytes_stream()` consumes the response by value.)
    #[cfg_attr(target_arch = "wasm32", allow(unused_mut))]
    async fn read_body_capped(mut response: Response, cap: u64) -> Result<Vec<u8>, Error> {
        let mut data = Vec::new();
        // Native: incremental `chunk()` reads (no extra features needed).
        #[cfg(not(target_arch = "wasm32"))]
        while let Some(chunk) = response.chunk().await? {
            if chunk_exceeds_cap(data.len(), chunk.len(), cap) {
                return Err(Self::cap_exceeded_error(cap));
            }
            data.extend_from_slice(&chunk);
        }
        // wasm: `bytes_stream()` (the wasm backend has no `chunk()`), driven
        // with `StreamExt` — also incremental, so the abort still fires
        // before the body is fully resident.
        #[cfg(target_arch = "wasm32")]
        {
            use futures_util::StreamExt;
            let mut stream = response.bytes_stream();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk?;
                if chunk_exceeds_cap(data.len(), chunk.len(), cap) {
                    return Err(Self::cap_exceeded_error(cap));
                }
                data.extend_from_slice(&chunk);
            }
        }
        Ok(data)
    }

    // nostr.blue: shared error for a body that overflowed its byte cap.
    fn cap_exceeded_error(cap: u64) -> Error {
        Error::new(
            ErrorKind::Invalid,
            format!("Response body exceeded the {cap}-byte limit"),
        )
    }

    /// Returns a default BlossomAuthorization object based on the parameters provided.
    fn default_auth<T>(
        &self,
        action: BlossomAuthorizationVerb,
        default_content: T,
        default_scope: BlossomAuthorizationScope,
    ) -> BlossomAuthorization
    where
        T: Into<String>,
    {
        let expiration_timestamp: Timestamp = Timestamp::now() + Duration::from_secs(300);
        BlossomAuthorization::new(
            default_content.into(),
            expiration_timestamp,
            action,
            default_scope,
        )
    }

    /// Updates a default BlossomAuthorization fixture with the provided options.
    pub fn update_authorization_fixture(
        default: &BlossomAuthorization,
        options: BlossomAuthorizationOptions,
    ) -> BlossomAuthorization {
        BlossomAuthorization {
            content: options.content.unwrap_or(default.content.clone()),
            expiration: options.expiration.unwrap_or(default.expiration),
            action: default.action,
            scope: default.scope.clone(),
        }
    }

    /// Helper function to build authorization header.
    ///
    /// <https://github.com/hzrd149/blossom/blob/master/buds/01.md>
    async fn build_auth_header<T>(
        signer: &T,
        authz: BlossomAuthorization,
    ) -> Result<HeaderValue, Error>
    where
        T: AsyncGetPublicKey + AsyncSignEvent,
    {
        let auth_event: Event = authz.finalize_async(signer).await?;
        let encoded_auth: String = URL_SAFE_NO_PAD.encode(auth_event.as_json());
        let value: String = format!("Nostr {}", encoded_auth);
        Ok(HeaderValue::try_from(value)?)
    }
}

/// Options for customizing BlossomAuthorization. All fields are optional.
#[derive(Debug, Clone, Default)]
pub struct BlossomAuthorizationOptions {
    /// A human readable string explaining to the user what the events intended use is
    pub content: Option<String>,
    /// A UNIX timestamp (in seconds) indicating when the authorization should be expired
    pub expiration: Option<Timestamp>,
    /// The type of action authorized by the user
    #[deprecated = "endpoint actions are fixed by BUD-11 and this field is ignored"]
    pub action: Option<BlossomAuthorizationVerb>,
    /// The scope of the authorization
    #[deprecated = "endpoint hash requirements are fixed by BUD-11 and this field is ignored"]
    pub scope: Option<BlossomAuthorizationScope>,
}

/// BUD-12 list filters and cursor pagination.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BlossomListOptions {
    /// Hash of the last blob from the previous page.
    pub cursor: Option<Sha256Hash>,
    /// Maximum number of descriptors to return.
    pub limit: Option<u32>,
    /// Deprecated lower upload-time bound.
    pub since: Option<Timestamp>,
    /// Deprecated upper upload-time bound.
    pub until: Option<Timestamp>,
}

/// Result of a BUD-05 or BUD-06 preflight request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlossomPreflight {
    /// The server indicates that the subsequent upload may proceed.
    Accepted,
    /// The server does not implement the optional preflight endpoint.
    Unsupported,
    /// The server requires one of the advertised payment methods.
    PaymentRequired(Vec<BlossomPaymentRequest>),
}

/// A BUD-07 payment request advertised by a server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlossomPaymentRequest {
    /// Payment method suffix from the `X-{method}` header.
    pub method: String,
    /// Encoded payment request supplied by the server.
    pub request: String,
}

/// Extracts BUD-07 payment requests (`X-Cashu`/`X-Lightning`) and the
/// `X-Reason` display text from a 402 response's headers.
// nostr.blue: BUD-07 payment-challenge parser, shared with nostr.blue's
// BUD-10 resolver (`src/stores/media/blossom_uri.rs::head_probe` calls this
// so the two copies cannot drift). Only `X-Cashu` and `X-Lightning` are
// payment methods defined today, so every other `X-*` header
// (X-Content-Type-Options, X-RateLimit-*, …) is ignored; `X-Reason` is
// returned separately for display. Header names are matched
// case-insensitively and empty values are skipped (a bare `X-Cashu:` is
// garbage, not a challenge).
pub fn parse_payment_request_headers(
    headers: &HeaderMap,
) -> (Vec<BlossomPaymentRequest>, Option<String>) {
    let mut requests = Vec::new();
    let mut reason = None;
    for (name, value) in headers {
        let lower = name.as_str().to_ascii_lowercase();
        let Some(method) = lower.strip_prefix("x-") else {
            continue;
        };
        let Ok(value) = value.to_str() else {
            continue;
        };
        match method {
            "reason" => reason = Some(value.to_owned()),
            "cashu" | "lightning" if !value.is_empty() => {
                requests.push(BlossomPaymentRequest {
                    method: method.to_owned(),
                    request: value.to_owned(),
                });
            }
            _ => {}
        }
    }
    (requests, reason)
}

impl BlossomPaymentRequest {
    /// Checks the payment method's required transport encoding.
    pub fn is_valid(&self) -> bool {
        // nostr.blue: upstream only accepts `creqA`, but NUT-24 requires
        // wallets to support the NUT-26 bech32m `creqb` encoding as well.
        let request = self.request.to_ascii_lowercase();
        match self.method.as_str() {
            "cashu" => request.starts_with("creqa") || request.starts_with("creqb1"),
            "lightning" => request.starts_with("ln") && self.request.is_ascii(),
            _ => !self.method.is_empty() && !self.request.is_empty(),
        }
    }
}

/// A BUD-07 payment proof to attach to a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlossomPaymentProof {
    /// Payment method suffix, such as `cashu` or `lightning`.
    pub method: String,
    /// Encoded proof defined by the selected payment method.
    pub proof: String,
}

impl BlossomPaymentProof {
    /// Checks the payment method's required proof encoding.
    pub fn is_valid(&self) -> bool {
        match self.method.as_str() {
            "cashu" => self.proof.starts_with("cashuB"),
            "lightning" => {
                self.proof.len() == 64 && self.proof.bytes().all(|byte| byte.is_ascii_hexdigit())
            }
            _ => !self.method.is_empty() && !self.proof.is_empty(),
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::task::JoinHandle;
    use tokio::time::{Duration, timeout};

    use super::*;

    async fn mock_server(response: &'static str) -> (Url, JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            let (mut stream, _) = timeout(Duration::from_secs(2), listener.accept())
                .await
                .expect("request deadline elapsed")
                .unwrap();
            let mut request = Vec::new();
            let mut buffer = [0_u8; 4096];
            loop {
                let read = timeout(Duration::from_secs(2), stream.read(&mut buffer))
                    .await
                    .expect("read deadline elapsed")
                    .unwrap();
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
                if let Some(headers_end) =
                    request.windows(4).position(|window| window == b"\r\n\r\n")
                {
                    let headers = String::from_utf8_lossy(&request[..headers_end]);
                    let content_length = headers
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    if request.len() >= headers_end + 4 + content_length {
                        break;
                    }
                }
            }
            stream.write_all(response.as_bytes()).await.unwrap();
            String::from_utf8(request).unwrap()
        });
        (
            Url::parse(&format!("http://{address}/ignored/path")).unwrap(),
            handle,
        )
    }

    #[tokio::test]
    async fn mirror_uses_root_endpoint_and_metadata_headers() {
        let hash = Sha256Hash::hash(b"blob");
        let body = format!(
            r#"{{"url":"https://cdn.example/{hash}.bin","sha256":"{hash}","size":4,"type":"application/octet-stream","uploaded":1}}"#
        );
        let response = Box::leak(
            format!(
                "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .into_boxed_str(),
        );
        let (base_url, request) = mock_server(response).await;
        let descriptor = BlobDescriptor {
            url: Url::parse(&format!("https://origin.example/{hash}.bin")).unwrap(),
            sha256: hash,
            size: 4,
            mime_type: "application/octet-stream".to_owned(),
            uploaded: Timestamp::from_secs(1),
            nip94: None,
        };

        BlossomClient::new(base_url)
            .mirror_blob(
                &descriptor,
                None,
                None::<&Keys>,
                Some(&BlossomPaymentProof {
                    method: "cashu".to_owned(),
                    proof: "cashuBproof".to_owned(),
                }),
            )
            .await
            .unwrap();

        let request = request.await.unwrap();
        let request_lowercase = request.to_ascii_lowercase();
        assert!(request.starts_with("PUT /mirror HTTP/1.1"));
        assert!(request_lowercase.contains(&format!("x-sha-256: {hash}")));
        assert!(request_lowercase.contains("x-content-length: 4"));
        assert!(request_lowercase.contains("x-cashu: cashubproof"));
        assert!(request.contains(&format!(r#"{{"url":"https://origin.example/{hash}.bin"}}"#)));
    }

    #[tokio::test]
    async fn preflight_returns_payment_challenges() {
        // nostr.blue: X-RateLimit-Remaining must NOT be parsed as a payment
        // method (regression guard for the whitelist).
        let response = "HTTP/1.1 402 Payment Required\r\nX-Cashu: creqArequest\r\nX-Reason: payment needed\r\nX-RateLimit-Remaining: 42\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        let (base_url, request) = mock_server(response).await;
        let hash = Sha256Hash::hash(b"blob");

        let result = BlossomClient::new(base_url)
            .upload_requirements(
                hash,
                4,
                Some("application/octet-stream"),
                None,
                None::<&Keys>,
            )
            .await
            .unwrap();

        assert_eq!(
            result,
            BlossomPreflight::PaymentRequired(vec![BlossomPaymentRequest {
                method: "cashu".to_owned(),
                request: "creqArequest".to_owned(),
            }])
        );
        let request = request.await.unwrap().to_ascii_lowercase();
        assert!(request.starts_with("head /upload http/1.1"));
        assert!(request.contains(&format!("x-sha-256: {hash}")));
        assert!(request.contains("x-content-length: 4"));
        assert!(request.contains("x-content-type: application/octet-stream"));
    }

    #[tokio::test]
    async fn list_page_sends_cursor_and_limit() {
        let response = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n[]";
        let (base_url, request) = mock_server(response).await;
        let keys = Keys::generate();
        let cursor = Sha256Hash::hash(b"cursor");

        let page = BlossomClient::new(base_url)
            .list_blobs_page(
                &keys.public_key(),
                BlossomListOptions {
                    cursor: Some(cursor),
                    limit: Some(25),
                    ..Default::default()
                },
                None,
                None::<&Keys>,
                None,
            )
            .await
            .unwrap();

        assert!(page.is_empty());
        let request = request.await.unwrap();
        assert!(request.starts_with(&format!(
            "GET /list/{}?cursor={cursor}&limit=25 HTTP/1.1",
            keys.public_key().to_hex()
        )));
    }

    #[tokio::test]
    async fn rejects_invalid_blob_report_before_request() {
        let event = EventBuilder::new(Kind::TextNote, "not a report")
            .finalize(&Keys::generate())
            .unwrap();
        let client = BlossomClient::new(Url::parse("http://127.0.0.1:1").unwrap());

        let error = client.report_blobs(&event).await.unwrap_err();

        assert_eq!(error.kind(), ErrorKind::Invalid);
    }

    #[tokio::test]
    async fn rejects_empty_upload_server_list() {
        let error =
            BlossomClient::upload_and_mirror(&[], b"blob".to_vec(), None, None, None::<&Keys>)
                .await
                .unwrap_err();

        assert_eq!(error.kind(), ErrorKind::Invalid);
    }

    #[tokio::test]
    async fn uri_resolution_verifies_hash_and_size() {
        let data = "blossom";
        let hash = Sha256Hash::hash(data.as_bytes());
        let response = Box::leak(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{data}",
                data.len()
            )
            .into_boxed_str(),
        );
        let (base_url, request) = mock_server(response).await;
        let mut uri = BlossomUri::new(hash, "bin");
        uri.size = Some(data.len() as u64);

        let resolved = BlossomClient::new(base_url.clone())
            .resolve_uri(&uri, [base_url], [])
            .await
            .unwrap();

        assert_eq!(resolved, data.as_bytes());
        assert!(
            request
                .await
                .unwrap()
                .starts_with(&format!("GET /{hash}.bin HTTP/1.1"))
        );
    }

    #[tokio::test]
    async fn uri_resolution_rejects_oversized_candidates() {
        // nostr.blue: the advertised Content-Length (7) exceeds the URI's
        // `sz` (3) — the candidate must be skipped before any body read.
        let data = "blossom";
        let hash = Sha256Hash::hash(data.as_bytes());
        let response = Box::leak(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{data}",
                data.len()
            )
            .into_boxed_str(),
        );
        let (base_url, _request) = mock_server(response).await;
        let mut uri = BlossomUri::new(hash, "bin");
        uri.size = Some(3);

        let result = BlossomClient::new(base_url.clone())
            .resolve_uri(&uri, [base_url], [])
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn uri_resolution_aborts_stream_past_cap() {
        // nostr.blue: no Content-Length header — the cap must be enforced
        // while streaming, not after the body is resident.
        let data = "blossom";
        let hash = Sha256Hash::hash(data.as_bytes());
        let response = Box::leak(
            format!("HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n{data}").into_boxed_str(),
        );
        let (base_url, _request) = mock_server(response).await;
        let mut uri = BlossomUri::new(hash, "bin");
        uri.size = Some(3);

        let result = BlossomClient::new(base_url.clone())
            .resolve_uri(&uri, [base_url], [])
            .await;

        assert!(result.is_err());
    }

    // nostr.blue: 402 parser whitelist — construct header maps directly,
    // no network.
    #[test]
    fn payment_headers_only_accept_known_methods() {
        let mut headers = HeaderMap::new();
        headers.insert("X-Cashu", HeaderValue::from_static("creqArequest"));
        headers.insert("X-Lightning", HeaderValue::from_static("lnbc100n1payload"));
        headers.insert(
            "X-Content-Type-Options",
            HeaderValue::from_static("nosniff"),
        );
        headers.insert("X-RateLimit-Remaining", HeaderValue::from_static("42"));
        headers.insert("X-Reason", HeaderValue::from_static("upload costs money"));

        let (requests, reason) = parse_payment_request_headers(&headers);

        assert_eq!(reason.as_deref(), Some("upload costs money"));
        assert_eq!(requests.len(), 2, "only cashu + lightning are methods");
        assert!(requests.iter().any(|r| r.method == "cashu"));
        assert!(requests.iter().any(|r| r.method == "lightning"));
    }

    #[test]
    fn payment_headers_skip_empty_values() {
        let mut headers = HeaderMap::new();
        headers.insert("X-Cashu", HeaderValue::from_static(""));
        headers.insert("X-Lightning", HeaderValue::from_static("lnbc1"));

        let (requests, reason) = parse_payment_request_headers(&headers);

        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].method, "lightning");
        assert!(reason.is_none());
    }

    #[test]
    fn fetch_cap_decisions() {
        // Cap selection: expected size wins, clamped to the hard cap.
        assert_eq!(effective_fetch_cap(None, 100), 100);
        assert_eq!(effective_fetch_cap(Some(40), 100), 40);
        assert_eq!(effective_fetch_cap(Some(400), 100), 100);
        // Running cap: exactly reaching the cap is fine, one byte over aborts.
        assert!(!chunk_exceeds_cap(30, 10, 40));
        assert!(!chunk_exceeds_cap(0, 40, 40));
        assert!(chunk_exceeds_cap(31, 10, 40));
        assert!(chunk_exceeds_cap(0, 41, 40));
        // No overflow on hostile lengths.
        assert!(!chunk_exceeds_cap(usize::MAX, usize::MAX, u64::MAX));
    }

    #[test]
    fn descriptor_urls_must_match_server_origin_and_scheme() {
        let server = Url::parse("https://blossom.example").unwrap();

        let ok = Url::parse("https://blossom.example/media/abc.png").unwrap();
        assert!(BlossomClient::validate_descriptor_url(&ok, &server).is_ok());

        let cdn = Url::parse("https://cdn.example/abc.png").unwrap();
        assert!(BlossomClient::validate_descriptor_url(&cdn, &server).is_err());

        let downgrade = Url::parse("http://blossom.example/abc.png").unwrap();
        assert!(BlossomClient::validate_descriptor_url(&downgrade, &server).is_err());

        let data = Url::parse("data:image/png;base64,AAAA").unwrap();
        assert!(BlossomClient::validate_descriptor_url(&data, &server).is_err());

        let script = Url::parse("javascript:alert(1)").unwrap();
        assert!(BlossomClient::validate_descriptor_url(&script, &server).is_err());
    }
}
