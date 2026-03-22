use anyhow::{anyhow, Result};
use hickory_proto::op::Message;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::time::timeout;
use tracing::{debug, warn};

use crate::config::UpstreamConfig;

/// Upstream protocol type, parsed from config address.
#[derive(Debug, Clone)]
pub enum UpstreamProto {
    /// Plain UDP (addr:port)
    Udp { addr: String },
    /// Plain TCP (tcp://addr:port)
    Tcp { addr: String },
    /// DNS-over-HTTPS (https://host/path)
    Doh { url: String },
    /// DNS-over-HTTPS/3 (h3://host/path) - falls back to DoH/2
    Doh3 { url: String },
}

pub struct Upstream {
    proto: UpstreamProto,
    weight: u32,
    http_client: Option<reqwest::Client>,
    queries: AtomicU64,
    failures: AtomicU64,
}

impl Upstream {
    pub fn new(config: &UpstreamConfig) -> Result<Self> {
        let proto = parse_upstream_addr(&config.addr)?;
        let http_client = match &proto {
            UpstreamProto::Doh { .. } | UpstreamProto::Doh3 { .. } => {
                let client = reqwest::Client::builder()
                    .timeout(Duration::from_secs(5))
                    .pool_max_idle_per_host(4)
                    .build()?;
                Some(client)
            }
            _ => None,
        };

        Ok(Self {
            proto,
            weight: config.weight,
            http_client,
            queries: AtomicU64::new(0),
            failures: AtomicU64::new(0),
        })
    }

    pub async fn exchange(&self, query: &Message, timeout_dur: Duration) -> Result<Message> {
        self.queries.fetch_add(1, Ordering::Relaxed);

        let result = match &self.proto {
            UpstreamProto::Udp { addr } => {
                timeout(timeout_dur, self.exchange_udp(query, addr)).await?
            }
            UpstreamProto::Tcp { addr } => {
                timeout(timeout_dur, self.exchange_tcp(query, addr)).await?
            }
            UpstreamProto::Doh { url } => {
                timeout(timeout_dur, self.exchange_doh(query, url)).await?
            }
            UpstreamProto::Doh3 { url } => {
                // h3 prefix is converted to https for the actual request
                let https_url = url.replacen("h3://", "https://", 1);
                timeout(timeout_dur, self.exchange_doh(query, &https_url)).await?
            }
        };

        if result.is_err() {
            self.failures.fetch_add(1, Ordering::Relaxed);
        }
        result
    }

    async fn exchange_udp(&self, query: &Message, addr: &str) -> Result<Message> {
        let wire = query.to_vec()?;
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        socket.connect(addr).await?;
        socket.send(&wire).await?;

        let mut buf = vec![0u8; 4096];
        let len = socket.recv(&mut buf).await?;
        buf.truncate(len);

        let response = Message::from_vec(&buf)?;

        // If truncated, retry over TCP
        if response.truncated() {
            debug!("UDP response truncated, retrying over TCP for {}", addr);
            return self.exchange_tcp(query, addr).await;
        }

        Ok(response)
    }

    async fn exchange_tcp(&self, query: &Message, addr: &str) -> Result<Message> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpStream;

        let wire = query.to_vec()?;
        let mut stream = TcpStream::connect(addr).await?;

        // DNS TCP: 2-byte length prefix
        let len = (wire.len() as u16).to_be_bytes();
        stream.write_all(&len).await?;
        stream.write_all(&wire).await?;

        // Read response length
        let mut len_buf = [0u8; 2];
        stream.read_exact(&mut len_buf).await?;
        let resp_len = u16::from_be_bytes(len_buf) as usize;

        let mut resp_buf = vec![0u8; resp_len];
        stream.read_exact(&mut resp_buf).await?;

        let response = Message::from_vec(&resp_buf)?;
        Ok(response)
    }

    async fn exchange_doh(&self, query: &Message, url: &str) -> Result<Message> {
        let client = self
            .http_client
            .as_ref()
            .ok_or_else(|| anyhow!("no HTTP client"))?;

        let wire = query.to_vec()?;

        let resp = client
            .post(url)
            .header("content-type", "application/dns-message")
            .header("accept", "application/dns-message")
            .body(wire)
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(anyhow!("DoH request failed with status: {}", resp.status()));
        }

        let body = resp.bytes().await?;
        let response = Message::from_vec(&body)?;
        Ok(response)
    }

    pub fn stats(&self) -> UpstreamStats {
        UpstreamStats {
            addr: match &self.proto {
                UpstreamProto::Udp { addr } => format!("udp://{}", addr),
                UpstreamProto::Tcp { addr } => format!("tcp://{}", addr),
                UpstreamProto::Doh { url } => url.clone(),
                UpstreamProto::Doh3 { url } => url.clone(),
            },
            queries: self.queries.load(Ordering::Relaxed),
            failures: self.failures.load(Ordering::Relaxed),
            weight: self.weight,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct UpstreamStats {
    pub addr: String,
    pub queries: u64,
    pub failures: u64,
    pub weight: u32,
}

/// A group of upstreams with weighted random selection.
pub struct UpstreamGroup {
    upstreams: Vec<Arc<Upstream>>,
    name: String,
}

impl UpstreamGroup {
    pub fn new(name: &str, configs: &[UpstreamConfig]) -> Result<Self> {
        let mut upstreams = Vec::new();
        for cfg in configs {
            upstreams.push(Arc::new(Upstream::new(cfg)?));
        }
        if upstreams.is_empty() {
            return Err(anyhow!("no upstreams configured for group '{}'", name));
        }
        Ok(Self {
            upstreams,
            name: name.to_string(),
        })
    }

    /// Query an upstream with weighted random selection.
    /// Tries up to 2 upstreams on failure.
    pub async fn exchange(&self, query: &Message, timeout_dur: Duration) -> Result<Message> {
        let selected = self.weighted_select();
        let max_retries = self.upstreams.len().min(2);

        for attempt in 0..max_retries {
            let idx = (selected + attempt) % self.upstreams.len();
            let upstream = &self.upstreams[idx];

            match upstream.exchange(query, timeout_dur).await {
                Ok(resp) => return Ok(resp),
                Err(e) => {
                    warn!(
                        group = %self.name,
                        upstream = %idx,
                        attempt = %attempt,
                        "upstream query failed: {}",
                        e
                    );
                    if attempt + 1 >= max_retries {
                        return Err(e);
                    }
                }
            }
        }
        Err(anyhow!("all upstreams failed for group '{}'", self.name))
    }

    fn weighted_select(&self) -> usize {
        if self.upstreams.len() == 1 {
            return 0;
        }
        let weights: Vec<u32> = self.upstreams.iter().map(|u| u.weight).collect();
        let total: u32 = weights.iter().sum();
        let mut rng = rand::thread_rng();
        let r = rand::Rng::gen_range(&mut rng, 0..total);
        let mut cumulative = 0;
        for (i, w) in weights.iter().enumerate() {
            cumulative += w;
            if r < cumulative {
                return i;
            }
        }
        0
    }

    pub fn stats(&self) -> Vec<UpstreamStats> {
        self.upstreams.iter().map(|u| u.stats()).collect()
    }
}

fn parse_upstream_addr(addr: &str) -> Result<UpstreamProto> {
    if addr.starts_with("https://") {
        Ok(UpstreamProto::Doh {
            url: addr.to_string(),
        })
    } else if addr.starts_with("h3://") {
        Ok(UpstreamProto::Doh3 {
            url: addr.to_string(),
        })
    } else if let Some(tcp_addr) = addr.strip_prefix("tcp://") {
        Ok(UpstreamProto::Tcp {
            addr: tcp_addr.to_string(),
        })
    } else {
        // Plain address: treat as UDP
        Ok(UpstreamProto::Udp {
            addr: addr.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_upstream_addr() {
        match parse_upstream_addr("8.8.8.8:53").unwrap() {
            UpstreamProto::Udp { addr } => assert_eq!(addr, "8.8.8.8:53"),
            _ => panic!("expected UDP"),
        }

        match parse_upstream_addr("tcp://8.8.8.8:53").unwrap() {
            UpstreamProto::Tcp { addr } => assert_eq!(addr, "8.8.8.8:53"),
            _ => panic!("expected TCP"),
        }

        match parse_upstream_addr("https://dns.google/dns-query").unwrap() {
            UpstreamProto::Doh { url } => assert_eq!(url, "https://dns.google/dns-query"),
            _ => panic!("expected DoH"),
        }

        match parse_upstream_addr("h3://dns.google/dns-query").unwrap() {
            UpstreamProto::Doh3 { url } => assert_eq!(url, "h3://dns.google/dns-query"),
            _ => panic!("expected DoH3"),
        }
    }
}
