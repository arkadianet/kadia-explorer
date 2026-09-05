use crate::{BlockSource, SourceError};
use std::sync::Arc;
use tracing::{info, warn};
use xp_types::Hash32;

/// A [`BlockSource`] that reads the chain from a primary source but will fetch a *block body*
/// from a second source when the primary cannot produce it.
///
/// Only [`BlockSource::full_block_json`] is doubled up. `best_height`, `header_id_at` and
/// `genesis_boxes_json` delegate to the primary alone, so the chain the index follows — which
/// height holds which header, and how far it goes — is still decided entirely by the primary.
/// The fallback can never move the index onto its own chain.
///
/// That is also why the fetched body needs no extra validation: it is requested *by header
/// id*, an id the primary announced, and any node that answers `/blocks/{id}` at all answers
/// with the block having that id. A fallback serving a different block would have to serve it
/// under the primary's id, at which point the body fails to decode into that header downstream
/// (`xp-ingest` decodes it and `xp-store` checks its parent linkage) rather than being applied.
///
/// The motivating case: the primary Rust node returns 404 for `/blocks/{id}` on blocks whose
/// header it happily serves from `/blocks/at/{height}` (a node-side parse bug), which stalls
/// ingest at that height forever.
pub struct Fallback {
    primary: Arc<dyn BlockSource>,
    fallback: Arc<dyn BlockSource>,
    name: String,
}

impl Fallback {
    pub fn new(primary: Arc<dyn BlockSource>, fallback: impl BlockSource + 'static) -> Fallback {
        let name = format!("{} (+fallback)", primary.name());
        Fallback {
            primary,
            fallback: Arc::new(fallback),
            name,
        }
    }
}

#[async_trait::async_trait]
impl BlockSource for Fallback {
    fn name(&self) -> &str {
        &self.name
    }

    async fn best_height(&self) -> Result<u32, SourceError> {
        self.primary.best_height().await
    }

    async fn header_id_at(&self, height: u32) -> Result<Option<Hash32>, SourceError> {
        self.primary.header_id_at(height).await
    }

    async fn full_block_json(&self, id: &Hash32) -> Result<Option<String>, SourceError> {
        let primary = self.primary.full_block_json(id).await;
        if let Ok(Some(json)) = primary {
            return Ok(Some(json));
        }
        match self.fallback.full_block_json(id).await {
            Ok(Some(json)) => {
                // The height is only in the body; parsing it here costs one extra JSON pass on
                // a path that is rare by construction, and makes the log line usable against
                // the chain rather than just against an opaque id.
                let height = serde_json::from_str::<serde_json::Value>(&json)
                    .ok()
                    .and_then(|v| v.get("header")?.get("height")?.as_u64());
                info!(
                    id = %xp_types::hex32(id),
                    height,
                    source = %self.fallback.name(),
                    "fetched block body from fallback source"
                );
                Ok(Some(json))
            }
            // The fallback added nothing: report exactly what the primary said, so a caller
            // still distinguishes "no such block" from "the primary is unreachable".
            Ok(None) => primary,
            Err(e) => {
                warn!(
                    id = %xp_types::hex32(id),
                    source = %self.fallback.name(),
                    error = %e,
                    "fallback source could not serve the block body either"
                );
                primary
            }
        }
    }

    async fn genesis_boxes_json(&self) -> Result<String, SourceError> {
        self.primary.genesis_boxes_json().await
    }
}
