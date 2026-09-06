use crate::{BlockSource, SourceError};
use std::sync::Once;
use std::time::Duration;
use xp_types::Hash32;

/// Guards the "this node has no chainSlice" warning: it is a property of the node, not of the
/// height, so it is said once per process instead of on every one of hundreds of thousands of
/// header lookups.
static NO_CHAIN_SLICE_WARNED: Once = Once::new();

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

    /// The pre-`chainSlice` way to name a height's header, kept only for nodes that lack the
    /// endpoint: `/blocks/at/{h}`'s first id. See [`BlockSource::header_id_at`] for why this
    /// is not good enough on a node that holds competing blocks.
    async fn header_id_at_via_blocks_at(&self, height: u32) -> Result<Option<Hash32>, SourceError> {
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
        // Debug, not warn: on a node holding orphans this fires at most heights, and the
        // once-per-process warning above already says the choice is unverified.
        if ids.len() > 1 {
            tracing::debug!(
                height,
                ids = ?ids,
                "multiple headers at height; taking the first (no chainSlice to disambiguate)"
            );
        }
        match ids.into_iter().next() {
            None => Ok(None),
            Some(id) => xp_types::parse_hex32(&id)
                .map(Some)
                .map_err(|e| SourceError::Decode(e.to_string())),
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
        // `/blocks/at/{h}` lists *every* header the node holds at `h`, orphans included, and
        // the best-chain one is not reliably first: at mainnet 1789057 the orphan came first,
        // was applied, and every later best-chain block then failed its parent check forever.
        // `chainSlice` answers from the best chain only. Its `fromHeight` is exclusive in
        // general, but `fromHeight == toHeight == h` returns exactly h's header (verified
        // against a live node), so that is the range we ask for.
        let url = format!(
            "{}/blocks/chainSlice?fromHeight={height}&toHeight={height}",
            self.base
        );
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(Self::map_reqwest_err)?;
        if !resp.status().is_success() {
            // Only a node without the endpoint falls back; a 5xx from a node that has it
            // would too, and then gets the same first-id behaviour it had before.
            NO_CHAIN_SLICE_WARNED.call_once(|| {
                tracing::warn!(
                    node = %self.base,
                    status = %resp.status(),
                    "node has no /blocks/chainSlice; falling back to /blocks/at, which cannot \
                     distinguish an orphan from the best-chain header at a height"
                );
            });
            return self.header_id_at_via_blocks_at(height).await;
        }
        let headers: Vec<serde_json::Value> = resp
            .json()
            .await
            .map_err(|e| SourceError::Decode(e.to_string()))?;
        // Filtered by height rather than taking `[0]`: a node whose range semantics differ
        // must yield "no header here", never a neighbouring height's id.
        let found = headers
            .iter()
            .find(|h| h.get("height").and_then(|v| v.as_u64()) == Some(u64::from(height)));
        let Some(header) = found else {
            return Ok(None);
        };
        let id = header
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SourceError::Decode(format!("GET {url}: header has no id")))?;
        xp_types::parse_hex32(id)
            .map(Some)
            .map_err(|e| SourceError::Decode(e.to_string()))
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
