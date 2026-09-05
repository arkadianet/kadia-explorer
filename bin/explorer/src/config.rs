//! TOML configuration for the `explorer` binary.

use serde::Deserialize;
use std::path::PathBuf;

fn default_poll_ms() -> u64 {
    xp_ingest::IngestConfig::default().poll_ms
}
fn default_bulk_batch() -> usize {
    xp_ingest::IngestConfig::default().bulk_batch
}
fn default_bulk_concurrency() -> usize {
    xp_ingest::IngestConfig::default().bulk_concurrency
}
fn default_durable_every() -> u32 {
    xp_ingest::IngestConfig::default().durable_every
}
fn default_tip_lag_for_bulk() -> u32 {
    xp_ingest::IngestConfig::default().tip_lag_for_bulk
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub data_dir: PathBuf,
    pub bind: String,
    pub source: SourceConfig,
    #[serde(default)]
    pub ingest: IngestSection,
}

#[derive(Debug, Deserialize)]
pub struct SourceConfig {
    pub kind: String,
    pub url: String,
}

#[derive(Debug, Deserialize)]
pub struct IngestSection {
    #[serde(default = "default_poll_ms")]
    pub poll_ms: u64,
    #[serde(default = "default_bulk_batch")]
    pub bulk_batch: usize,
    #[serde(default = "default_bulk_concurrency")]
    pub bulk_concurrency: usize,
    #[serde(default = "default_durable_every")]
    pub durable_every: u32,
    #[serde(default = "default_tip_lag_for_bulk")]
    pub tip_lag_for_bulk: u32,
}

impl Default for IngestSection {
    fn default() -> IngestSection {
        let d = xp_ingest::IngestConfig::default();
        IngestSection {
            poll_ms: d.poll_ms,
            bulk_batch: d.bulk_batch,
            bulk_concurrency: d.bulk_concurrency,
            durable_every: d.durable_every,
            tip_lag_for_bulk: d.tip_lag_for_bulk,
        }
    }
}

impl From<&IngestSection> for xp_ingest::IngestConfig {
    fn from(s: &IngestSection) -> xp_ingest::IngestConfig {
        xp_ingest::IngestConfig {
            poll_ms: s.poll_ms,
            bulk_batch: s.bulk_batch,
            bulk_concurrency: s.bulk_concurrency,
            durable_every: s.durable_every,
            tip_lag_for_bulk: s.tip_lag_for_bulk,
        }
    }
}

impl Config {
    pub fn parse(text: &str) -> anyhow::Result<Config> {
        let cfg: Config = toml::from_str(text)?;
        if cfg.source.kind != "rust_node" {
            anyhow::bail!(
                "unsupported [source] kind {:?}: only \"rust_node\" is supported",
                cfg.source.kind
            );
        }
        Ok(cfg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = include_str!("../../../explorer.example.toml");

    #[test]
    fn parses_example_toml() {
        let cfg = Config::parse(EXAMPLE).expect("example config should parse");
        assert_eq!(cfg.data_dir, PathBuf::from("./data"));
        assert_eq!(cfg.bind, "127.0.0.1:8090");
        assert_eq!(cfg.source.kind, "rust_node");
        assert_eq!(cfg.source.url, "http://127.0.0.1:9063");
        assert_eq!(cfg.ingest.poll_ms, 500);
        assert_eq!(cfg.ingest.bulk_batch, 64);
        assert_eq!(cfg.ingest.bulk_concurrency, 8);
        assert_eq!(cfg.ingest.durable_every, 256);
        assert_eq!(cfg.ingest.tip_lag_for_bulk, 64);
    }

    #[test]
    fn defaults_applied_when_ingest_omitted() {
        let text = r#"
            data_dir = "./data"
            bind = "127.0.0.1:8090"
            [source]
            kind = "rust_node"
            url = "http://127.0.0.1:9063"
        "#;
        let cfg = Config::parse(text).expect("config without [ingest] should parse");
        let default = xp_ingest::IngestConfig::default();
        assert_eq!(cfg.ingest.poll_ms, default.poll_ms);
        assert_eq!(cfg.ingest.bulk_batch, default.bulk_batch);
        assert_eq!(cfg.ingest.bulk_concurrency, default.bulk_concurrency);
        assert_eq!(cfg.ingest.durable_every, default.durable_every);
        assert_eq!(cfg.ingest.tip_lag_for_bulk, default.tip_lag_for_bulk);
    }

    #[test]
    fn unknown_source_kind_rejected() {
        let text = r#"
            data_dir = "./data"
            bind = "127.0.0.1:8090"
            [source]
            kind = "scala_node"
            url = "http://127.0.0.1:9053"
        "#;
        let err = Config::parse(text).expect_err("unknown source kind must be rejected");
        assert!(err.to_string().contains("scala_node"));
    }
}
