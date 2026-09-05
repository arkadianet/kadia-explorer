use crate::{BlockSource, SourceError};
use std::time::Duration;
use xp_types::Hash32;

/// `BlockSource` backed by a standard Ergo node's REST API (Rust or Scala reference node —
/// the endpoint shapes used here are identical on both).
pub struct RustNode {
    base: String,
    http: reqwest::Client,
}

impl RustNode {
    pub fn new(base_url: &str) -> RustNode {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(10))
            .gzip(true)
            .build()
            .expect("reqwest client build");
        RustNode {
            base: base_url.trim_end_matches('/').to_owned(),
            http,
        }
    }

    fn map_reqwest_err(e: reqwest::Error) -> SourceError {
        if e.is_connect() || e.is_timeout() {
            SourceError::Unavailable
        } else {
            SourceError::Http(e.to_string())
        }
    }
}

#[async_trait::async_trait]
impl BlockSource for RustNode {
    fn name(&self) -> &str {
        &self.base
    }

    async fn best_height(&self) -> Result<u32, SourceError> {
        let url = format!("{}/info", self.base);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(Self::map_reqwest_err)?;
        if !resp.status().is_success() {
            return Err(SourceError::Http(format!(
                "GET {url}: status {}",
                resp.status()
            )));
        }
        let v: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| SourceError::Decode(e.to_string()))?;
        v.get("fullHeight")
            .and_then(|h| h.as_u64())
            .map(|h| h as u32)
            .ok_or_else(|| SourceError::Decode("missing fullHeight".to_owned()))
    }

    async fn header_id_at(&self, height: u32) -> Result<Option<Hash32>, SourceError> {
        let url = format!("{}/blocks/at/{height}", self.base);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(Self::map_reqwest_err)?;
        if !resp.status().is_success() {
            return Err(SourceError::Http(format!(
                "GET {url}: status {}",
                resp.status()
            )));
        }
        let ids: Vec<String> = resp
            .json()
            .await
            .map_err(|e| SourceError::Decode(e.to_string()))?;
        // More than one header at a height means the node is holding competing blocks. Its
        // convention is to list the best-chain one first (see `BlockSource::header_id_at`),
        // so we take the first — but say so, since which chain the index follows then
        // depends on that convention rather than on anything we verify.
        if ids.len() > 1 {
            tracing::warn!(
                height,
                ids = ?ids,
                "multiple headers at height; taking the first (node best-chain convention)"
            );
        }
        match ids.into_iter().next() {
            None => Ok(None),
            Some(id) => xp_types::parse_hex32(&id)
                .map(Some)
                .map_err(|e| SourceError::Decode(e.to_string())),
        }
    }

    async fn full_block_json(&self, id: &Hash32) -> Result<Option<String>, SourceError> {
        let url = format!("{}/blocks/{}", self.base, xp_types::hex32(id));
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(Self::map_reqwest_err)?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !resp.status().is_success() {
            return Err(SourceError::Http(format!(
                "GET {url}: status {}",
                resp.status()
            )));
        }
        resp.text()
            .await
            .map(Some)
            .map_err(|e| SourceError::Decode(e.to_string()))
    }

    async fn genesis_boxes_json(&self) -> Result<String, SourceError> {
        let url = format!("{}/utxo/genesis", self.base);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(Self::map_reqwest_err)?;
        if !resp.status().is_success() {
            return Err(SourceError::Http(format!(
                "GET {url}: status {}",
                resp.status()
            )));
        }
        resp.text()
            .await
            .map_err(|e| SourceError::Decode(e.to_string()))
    }
}
