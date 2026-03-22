use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Listen address for DNS server (e.g. "0.0.0.0:53")
    pub listen: String,

    /// Protocol to listen on: "udp", "tcp", or "both"
    #[serde(default = "default_listen_proto")]
    pub listen_proto: String,

    /// HTTP API listen address (e.g. "0.0.0.0:8080")
    #[serde(default = "default_api_listen")]
    pub api_listen: String,

    /// China domain list file path (one domain per line, supports suffix matching)
    pub china_domains_file: String,

    /// China upstream DNS servers
    pub china_upstreams: Vec<UpstreamConfig>,

    /// Global (non-China) upstream DNS servers
    pub global_upstreams: Vec<UpstreamConfig>,

    /// Cache configuration
    #[serde(default)]
    pub cache: CacheConfig,

    /// Prefer IPv4 over IPv6 (filter AAAA if A exists)
    #[serde(default)]
    pub prefer_v4: bool,

    /// Query timeout in milliseconds
    #[serde(default = "default_query_timeout_ms")]
    pub query_timeout_ms: u64,

    /// Log level: "trace", "debug", "info", "warn", "error"
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamConfig {
    /// Upstream address. Examples:
    ///   "8.8.8.8:53" (UDP/TCP)
    ///   "tcp://8.8.8.8:53" (TCP only)
    ///   "https://dns.google/dns-query" (DoH)
    ///   "h3://dns.google/dns-query" (DoH3)
    pub addr: String,

    /// Optional weight for load balancing (default: 1)
    #[serde(default = "default_weight")]
    pub weight: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Maximum cache size in bytes (default 32MB)
    #[serde(default = "default_cache_max_bytes")]
    pub max_bytes: usize,

    /// Minimum TTL in seconds (floor for cache entries)
    #[serde(default = "default_min_ttl")]
    pub min_ttl: u32,

    /// Maximum TTL in seconds (cap for cache entries)
    #[serde(default = "default_max_ttl")]
    pub max_ttl: u32,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_bytes: default_cache_max_bytes(),
            min_ttl: default_min_ttl(),
            max_ttl: default_max_ttl(),
        }
    }
}

fn default_listen_proto() -> String {
    "both".to_string()
}
fn default_api_listen() -> String {
    "0.0.0.0:8053".to_string()
}
fn default_cache_max_bytes() -> usize {
    32 * 1024 * 1024 // 32MB
}
fn default_min_ttl() -> u32 {
    60
}
fn default_max_ttl() -> u32 {
    86400
}
fn default_weight() -> u32 {
    1
}
fn default_query_timeout_ms() -> u64 {
    5000
}
fn default_log_level() -> String {
    "info".to_string()
}

impl Config {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = serde_json::from_str(&content)?;
        Ok(config)
    }

    pub fn example() -> Self {
        Config {
            listen: "0.0.0.0:53".to_string(),
            listen_proto: "both".to_string(),
            api_listen: "0.0.0.0:8053".to_string(),
            china_domains_file: "china_domains.txt".to_string(),
            china_upstreams: vec![
                UpstreamConfig {
                    addr: "119.29.29.29:53".to_string(),
                    weight: 1,
                },
                UpstreamConfig {
                    addr: "223.5.5.5:53".to_string(),
                    weight: 1,
                },
            ],
            global_upstreams: vec![
                UpstreamConfig {
                    addr: "https://1.1.1.1/dns-query".to_string(),
                    weight: 1,
                },
                UpstreamConfig {
                    addr: "https://8.8.8.8/dns-query".to_string(),
                    weight: 1,
                },
            ],
            cache: CacheConfig::default(),
            prefer_v4: false,
            query_timeout_ms: 5000,
            log_level: "info".to_string(),
        }
    }
}
