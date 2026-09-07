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
    #[serde(default)]
    pub api: ApiSection,
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ApiSection {
    pub max_inflight_reads: u32,
    pub trusted_proxies: Vec<String>,
    pub rate_limit: RateLimitSection,
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RateLimitSection {
    pub per_second: u32,
    pub burst: u32,
    pub allowlist: Vec<String>,
}

impl Default for ApiSection {
    fn default() -> ApiSection {
        ApiSection {
            max_inflight_reads: 32,
            trusted_proxies: vec!["127.0.0.1".into(), "::1".into()],
            rate_limit: RateLimitSection::default(),
        }
    }
}

impl Default for RateLimitSection {
    fn default() -> RateLimitSection {
        RateLimitSection {
            per_second: 10,
            burst: 30,
            allowlist: Vec::new(),
        }
    }
}

impl TryFrom<&ApiSection> for xp_api::ApiConfig {
    type Error = String;
    fn try_from(s: &ApiSection) -> Result<xp_api::ApiConfig, String> {
        Ok(xp_api::ApiConfig {
            per_second: s.rate_limit.per_second,
            burst: s.rate_limit.burst,
            allowlist: xp_api::Allowlist::parse(&s.rate_limit.allowlist)
                .map_err(|e| format!("[api.rate_limit] allowlist: {e}"))?,
            trusted_proxies: xp_api::Allowlist::parse(&s.trusted_proxies)
                .map_err(|e| format!("[api] trusted_proxies: {e}"))?,
            max_inflight_reads: s.max_inflight_reads.max(1),
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct SourceConfig {
    pub kind: String,
    pub url: String,
    /// Optional second node, used *only* to fetch block bodies `url` announces but will not
    /// serve. Unset (the default) means no fallback and no behaviour change.
    #[serde(default)]
    pub fallback_url: Option<String>,
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
        assert_eq!(
            cfg.source.fallback_url, None,
            "the example's fallback_url is commented out: no fallback by default"
        );
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
    fn fallback_url_is_optional_and_parsed_when_present() {
        let text = r#"
            data_dir = "./data"
            bind = "127.0.0.1:8090"
            [source]
            kind = "rust_node"
            url = "http://127.0.0.1:9063"
            fallback_url = "https://node.ergo.watch"
        "#;
        let cfg = Config::parse(text).expect("config with fallback_url should parse");
        assert_eq!(
            cfg.source.fallback_url.as_deref(),
            Some("https://node.ergo.watch")
        );
    }

    #[test]
    fn api_section_defaults_when_absent() {
        let cfg = Config::parse(
            r#"
        data_dir = "/tmp/x"
        bind = "127.0.0.1:1"
        [source]
        kind = "rust_node"
        url = "http://127.0.0.1:9053"
    "#,
        )
        .unwrap();
        let api = xp_api::ApiConfig::try_from(&cfg.api).unwrap();
        assert_eq!(
            (api.per_second, api.burst, api.max_inflight_reads),
            (10, 30, 32)
        );
        assert!(api.trusted_proxies.contains("127.0.0.1".parse().unwrap()));
        assert!(!api.allowlist.contains("1.1.1.1".parse().unwrap()));
    }

    #[test]
    fn api_section_parses_and_rejects_bad_cidr() {
        let good = Config::parse(
            r#"
        data_dir = "/tmp/x"
        bind = "127.0.0.1:1"
        [source]
        kind = "rust_node"
        url = "http://127.0.0.1:9053"
        [api]
        max_inflight_reads = 4
        trusted_proxies = ["10.0.0.1"]
        [api.rate_limit]
        per_second = 2
        burst = 5
        allowlist = ["203.0.113.0/24", "2001:db8::/32"]
    "#,
        )
        .unwrap();
        let api = xp_api::ApiConfig::try_from(&good.api).unwrap();
        assert_eq!(
            (api.per_second, api.burst, api.max_inflight_reads),
            (2, 5, 4)
        );
        assert!(api.allowlist.contains("203.0.113.9".parse().unwrap()));
        assert!(api.trusted_proxies.contains("10.0.0.1".parse().unwrap()));
        assert!(!api.trusted_proxies.contains("127.0.0.1".parse().unwrap()));

        let bad = Config::parse(
            r#"
        data_dir = "/tmp/x"
        bind = "127.0.0.1:1"
        [source]
        kind = "rust_node"
        url = "http://127.0.0.1:9053"
        [api.rate_limit]
        allowlist = ["1.2.3.4/40"]
    "#,
        )
        .unwrap();
        let err = xp_api::ApiConfig::try_from(&bad.api).unwrap_err();
        assert!(err.contains("1.2.3.4/40"), "{err}");
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

    #[test]
    fn api_section_rejects_unknown_keys() {
        let err = Config::parse(
            r#"
        data_dir = "/tmp/x"
        bind = "127.0.0.1:1"
        [source]
        kind = "rust_node"
        url = "http://127.0.0.1:9053"
        [api]
        max_inflight_read = 4
    "#,
        )
        .expect_err("typo in [api] must fail to parse");
        assert!(format!("{err:#}").contains("max_inflight_read"), "{err:#}");
    }
}
