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
            let status = resp.status();
            if matches!(
                status,
                reqwest::StatusCode::NOT_FOUND
                    | reqwest::StatusCode::METHOD_NOT_ALLOWED
                    | reqwest::StatusCode::NOT_IMPLEMENTED
            ) {
                return Err(SourceError::Capability(format!(
                    "GET {url}: status {status}; canonical selection requires /blocks/chainSlice; \
                     upgrade this node or configure a primary that supports /blocks/chainSlice; \
                     /blocks/at cannot establish canonicality"
                )));
            }
            return Err(SourceError::Http(format!("GET {url}: status {status}")));
        }
        let headers: Vec<serde_json::Value> = resp
            .json()
            .await
            .map_err(|e| SourceError::Decode(e.to_string()))?;
        canonical_id(&headers, height)
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

/// Validate every entry before selecting: a contradictory response must never win by order.
fn canonical_id(headers: &[serde_json::Value], height: u32) -> Result<Option<Hash32>, SourceError> {
    let mut by_height = std::collections::HashMap::new();
    for header in headers {
        let at = header
            .get("height")
            .and_then(|v| v.as_u64())
            .and_then(|v| u32::try_from(v).ok())
            .ok_or_else(|| SourceError::Decode("chainSlice header has invalid height".into()))?;
        let id = header
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SourceError::Decode("chainSlice header has no id".into()))?;
        let id = xp_types::parse_hex32(id).map_err(|e| SourceError::Decode(e.to_string()))?;
        if let Some(previous) = by_height.insert(at, id) {
            if previous != id {
                return Err(SourceError::Decode(format!(
                    "contradictory chainSlice headers at height {at}"
                )));
            }
        }
    }
    // Nodes may clamp above-tip requests to their tip. Never return that neighbouring id.
    Ok(by_height.get(&height).copied())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn canonical_entries_fail_closed() {
        let id = xp_types::hex32(&[1; 32]);
        for entries in [
            json!([{"height": 42}]),
            json!([{"height": 42, "id": "bad"}]),
            json!([{"id": id}]),
            json!([{"height": "42", "id": id}]),
            json!([{"height": 4294967296u64, "id": id}]),
            json!([{"height": 42, "id": id}, {"height": 42, "id": xp_types::hex32(&[2; 32])}]),
            json!([{"height": 42, "id": id}, null]),
        ] {
            assert!(
                matches!(
                    canonical_id(entries.as_array().unwrap(), 42),
                    Err(SourceError::Decode(_))
                ),
                "{entries}"
            );
        }
    }

    #[test]
    fn canonical_selection_requires_requested_height() {
        let entries = vec![json!({"height": 41, "id": xp_types::hex32(&[1; 32])})];
        assert_eq!(canonical_id(&entries, 42).unwrap(), None);
        assert_eq!(canonical_id(&entries, 41).unwrap(), Some([1; 32]));
        assert_eq!(canonical_id(&[], 42).unwrap(), None);
    }
}
