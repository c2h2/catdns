# catdns

A fast DNS forwarder with China/global domain routing, written in Rust.

## Features

- **Split DNS routing**: China domains → China upstreams (119.29.29.29, 223.5.5.5), everything else → global upstreams (Cloudflare/Google DoH)
- **MixMatcher domain matching** (same as mosdns): 4 strategies — `full:` (exact), `domain:` (trie suffix, default), `keyword:` (substring), `regexp:` (regex). Match order: full → domain → keyword → regexp
- **Multiple upstream protocols**: UDP, TCP, DNS-over-HTTPS (HTTP/2), DNS-over-HTTPS/3
- **Large-scale cache**: Sharded LRU (256 shards) with byte-size eviction (default 32MB) and minimal lock contention
- **IPv4/IPv6**: Full A and AAAA support, optional `prefer_v4` mode to suppress AAAA when A exists in cache
- **HTTP API**: Stats, query history, cache info, upstream health
- **Massive parallelism**: Tokio async runtime, one task per query

## Quick Start

```bash
# Build
./compile.sh

# Generate example config
./target/release/catdns --gen-config > config.json

# Edit config (set listen port, upstreams, domain list file)
vim config.json

# Run
./target/release/catdns -c config.json
```

## Build from Source

Requires Rust toolchain (rustc 1.70+).

```bash
# Debug build
cargo build

# Release build (optimized, LTO)
cargo build --release

# Run tests
cargo test
```

## Config (JSON)

```json
{
  "listen": "0.0.0.0:53",
  "listen_proto": "both",
  "api_listen": "0.0.0.0:8053",
  "china_domains_file": "china_domains.txt",
  "china_upstreams": [
    { "addr": "119.29.29.29:53", "weight": 1 },
    { "addr": "223.5.5.5:53", "weight": 1 }
  ],
  "global_upstreams": [
    { "addr": "https://1.1.1.1/dns-query", "weight": 1 },
    { "addr": "https://8.8.8.8/dns-query", "weight": 1 }
  ],
  "cache": { "max_bytes": 33554432, "min_ttl": 60, "max_ttl": 86400 },
  "prefer_v4": false,
  "query_timeout_ms": 5000,
  "log_level": "info"
}
```

### Config Fields

| Field | Default | Description |
|-------|---------|-------------|
| `listen` | — | DNS listen address (e.g. `0.0.0.0:53`) |
| `listen_proto` | `both` | `udp`, `tcp`, or `both` |
| `api_listen` | `0.0.0.0:8053` | HTTP API listen address |
| `china_domains_file` | — | Path to China domain list file |
| `china_upstreams` | — | Upstream servers for China domains |
| `global_upstreams` | — | Upstream servers for non-China domains |
| `cache.max_bytes` | `33554432` | Maximum cache size in bytes (default 32MB) |
| `cache.min_ttl` | `60` | Minimum TTL floor (seconds) |
| `cache.max_ttl` | `86400` | Maximum TTL cap (seconds) |
| `prefer_v4` | `false` | Suppress AAAA responses when A record is cached |
| `query_timeout_ms` | `5000` | Per-query upstream timeout |
| `log_level` | `info` | `trace`, `debug`, `info`, `warn`, `error` |

### Upstream Address Formats

| Format | Protocol |
|--------|----------|
| `8.8.8.8:53` | UDP (default) |
| `tcp://8.8.8.8:53` | TCP |
| `https://dns.google/dns-query` | DNS-over-HTTPS |
| `h3://dns.google/dns-query` | DNS-over-HTTPS/3 |

### China Domains File Format

One rule per line. Inline comments with `#`. Same syntax as mosdns domain_set files.

```text
# Suffix match (default, same as "domain:" prefix)
baidu.com
cn

# Explicit prefix types
domain:qq.com
full:exactly-this.example.com
keyword:tencent
regexp:^ad[sx]?\.
```

| Prefix | Match Type | Example |
|--------|-----------|---------|
| *(none)* | Suffix (trie) | `baidu.com` matches `www.baidu.com` |
| `domain:` | Suffix (trie) | Same as no prefix |
| `full:` | Exact | `full:example.com` only matches `example.com` |
| `keyword:` | Substring | `keyword:google` matches `mail.google.co.jp` |
| `regexp:` | Regex | `regexp:^ads?\.` matches `ad.tracker.com` |

The recommended source for a comprehensive China domain list is [dnsmasq-china-list](https://github.com/felixonmars/dnsmasq-china-list). To convert:

```bash
# Download and convert dnsmasq-china-list to plain domain list
curl -sL https://raw.githubusercontent.com/felixonmars/dnsmasq-china-list/master/accelerated-domains.china.conf \
  | sed -n 's|^server=/\(.*\)/.*|\1|p' > china_domains.txt
```

## HTTP API

| Endpoint | Description |
|----------|-------------|
| `GET /` | API index |
| `GET /stats` | Server stats: uptime, query counts (total/china/global), cache stats |
| `GET /history` | Last 1000 queries with timing, cache hit, china/global classification |
| `GET /cache/stats` | Cache hit/miss/eviction counts, hit rate, entry count |
| `GET /upstreams` | Per-upstream query/failure counts for both china and global groups |

### Example API Response (`/stats`)

```json
{
  "uptime_seconds": 3600,
  "handler": {
    "total_queries": 15234,
    "china_queries": 8912,
    "global_queries": 6322
  },
  "cache": {
    "hits": 12045,
    "misses": 3189,
    "inserts": 3189,
    "evictions": 0,
    "hit_rate": 0.79,
    "entries": 3189
  }
}
```

## Architecture

```
                        ┌─────────────────┐
                        │   DNS Query      │
                        └────────┬────────┘
                                 │
                    ┌────────────▼────────────┐
                    │     UDP/TCP Server      │
                    │  (tokio async, per-task) │
                    └────────────┬────────────┘
                                 │
                    ┌────────────▼────────────┐
                    │      Cache Lookup       │
                    │  (256-shard LRU, 32MB)  │
                    └──────┬──────────┬───────┘
                      hit  │          │ miss
                           │          │
                           │  ┌───────▼────────┐
                           │  │  MixMatcher     │
                           │  │  full→domain→   │
                           │  │  keyword→regexp  │
                           │  └──┬──────────┬──┘
                           │china│          │global
                           │     │          │
                    ┌──────▼─┐ ┌─▼────┐  ┌──▼──────┐
                    │Response│ │China │  │ Global  │
                    │        │ │  DNS │  │   DoH   │
                    └────────┘ └──────┘  └─────────┘
```
