//! Downloading the manifest (conditional GET with `If-None-Match`).

use std::time::Duration;

pub const MANIFEST_URL: &str = "https://raw.githubusercontent.com/mtkennerly/ludusavi-manifest/master/data/manifest.yaml";

/// Largest manifest accepted (decompressed); it is ~18 MB today.
const MAX_BYTES: u64 = 256 * 1024 * 1024;

pub enum FetchResult {
    /// The cached copy (matching ETag) is current.
    NotModified,
    Updated { body: Vec<u8>, etag: Option<String> },
}

/// Source of the manifest; tests use a fake one.
pub trait Fetcher {
    fn fetch(&self, etag: Option<&str>) -> Result<FetchResult, String>;
}

/// Downloads from GitHub over HTTPS.
pub struct HttpFetcher;

impl Fetcher for HttpFetcher {
    fn fetch(&self, etag: Option<&str>) -> Result<FetchResult, String> {
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(180)))
            .build();
        let agent = ureq::Agent::new_with_config(config);
        let mut request = agent
            .get(MANIFEST_URL)
            .header("User-Agent", concat!("save-files-manager/", env!("CARGO_PKG_VERSION")));
        if let Some(etag) = etag {
            request = request.header("If-None-Match", etag);
        }
        let mut response = request.call().map_err(|e| format!("download failed: {e}"))?;
        match response.status().as_u16() {
            304 => Ok(FetchResult::NotModified),
            200 => {
                let etag = response
                    .headers()
                    .get("etag")
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_string);
                let body = response
                    .body_mut()
                    .with_config()
                    .limit(MAX_BYTES)
                    .read_to_vec()
                    .map_err(|e| format!("download failed: {e}"))?;
                Ok(FetchResult::Updated { body, etag })
            }
            status => Err(format!("download failed: HTTP {status}")),
        }
    }
}
