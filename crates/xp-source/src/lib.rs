mod rust_node;

pub use rust_node::RustNode;

use xp_types::Hash32;

/// A source of raw block data, keyed by height and header id, as exposed by the standard
/// Ergo node REST API (identical shape on the Rust and Scala reference nodes).
#[async_trait::async_trait]
pub trait BlockSource: Send + Sync {
    /// A short, human-readable label for logging (e.g. the base URL).
    fn name(&self) -> &str;
    /// The chain's current best (tip) height, per `/info`'s `fullHeight`.
    async fn best_height(&self) -> Result<u32, SourceError>;
    /// The header id at `height` on the source's current best chain, or `None` if the
    /// source has no block at that height (yet, or ever, if it is beyond its own tip).
    ///
    /// A height can hold more than one header (competing blocks the node has seen and kept).
    /// The node's `/blocks/at/{h}` lists the best-chain header first, and implementations
    /// rely on that convention: they return the first id and log a warning when there is
    /// more than one. Callers get one id per height, so a fork is detected the usual way —
    /// by the id at a height changing between polls — not by inspecting a list here.
    async fn header_id_at(&self, height: u32) -> Result<Option<Hash32>, SourceError>;
    /// The raw block JSON body for header `id`, or `None` if the source doesn't have it.
    async fn full_block_json(&self, id: &Hash32) -> Result<Option<String>, SourceError>;
    /// The raw JSON array of the chain-spec genesis boxes (`/utxo/genesis`). These boxes are
    /// created by the chain spec rather than by any block, so they are the one piece of
    /// indexable state that cannot be reached through the block endpoints.
    async fn genesis_boxes_json(&self) -> Result<String, SourceError>;
}

#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("http: {0}")]
    Http(String),
    #[error("decode: {0}")]
    Decode(String),
    #[error("source unavailable")]
    Unavailable,
}
