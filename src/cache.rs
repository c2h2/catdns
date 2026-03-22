use hickory_proto::op::Message;
use lru::LruCache;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Sharded LRU cache for DNS responses.
/// Sharding reduces lock contention under high concurrency.
/// Eviction is based on total memory usage (bytes), not entry count.
const NUM_SHARDS: usize = 256;

pub struct DnsCache {
    shards: Vec<Mutex<Shard>>,
    max_bytes_per_shard: usize,
    min_ttl: u32,
    max_ttl: u32,
    stats: CacheStats,
}

struct Shard {
    lru: LruCache<CacheKey, CacheEntry>,
    current_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheKey {
    pub name: String,
    pub qtype: u16,
    pub qclass: u16,
}

struct CacheEntry {
    message: Vec<u8>, // Wire-format DNS message
    inserted_at: Instant,
    ttl: Duration,
    _original_ttl: u32,
    entry_bytes: usize, // Tracked size of this entry (key + value)
}

pub struct CacheStats {
    pub hits: AtomicU64,
    pub misses: AtomicU64,
    pub inserts: AtomicU64,
    pub evictions: AtomicU64,
}

impl CacheStats {
    fn new() -> Self {
        Self {
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            inserts: AtomicU64::new(0),
            evictions: AtomicU64::new(0),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CacheStatsSnapshot {
    pub hits: u64,
    pub misses: u64,
    pub inserts: u64,
    pub evictions: u64,
    pub hit_rate: f64,
    pub entries: usize,
    pub bytes_used: usize,
}

/// Estimate the memory cost of a cache entry (key + wire message + fixed overhead).
fn entry_size(key: &CacheKey, wire_len: usize) -> usize {
    // Key: String heap (name.len()) + 2x u16
    // Value: Vec heap (wire_len) + Instant(8/16) + Duration(12) + u32 + usize
    // Approximate fixed overhead per entry (LRU node pointers, HashMap slot, etc.)
    const FIXED_OVERHEAD: usize = 128;
    key.name.len() + 4 + wire_len + FIXED_OVERHEAD
}

impl DnsCache {
    pub fn new(max_bytes: usize, min_ttl: u32, max_ttl: u32) -> Self {
        let max_bytes_per_shard = (max_bytes / NUM_SHARDS).max(1);
        let shards = (0..NUM_SHARDS)
            .map(|_| {
                Mutex::new(Shard {
                    // Unbounded capacity — we manage eviction by byte size
                    lru: LruCache::unbounded(),
                    current_bytes: 0,
                })
            })
            .collect();

        Self {
            shards,
            max_bytes_per_shard,
            min_ttl,
            max_ttl,
            stats: CacheStats::new(),
        }
    }

    fn shard_index(key: &CacheKey) -> usize {
        let mut hash: u64 = 5381;
        for b in key.name.as_bytes() {
            hash = hash.wrapping_mul(33).wrapping_add(*b as u64);
        }
        hash = hash.wrapping_add(key.qtype as u64);
        (hash as usize) % NUM_SHARDS
    }

    /// Store a DNS response in cache.
    pub fn put(&self, key: CacheKey, message: &Message) {
        // Extract minimum TTL from answer/authority/additional sections
        let ttl = self.effective_ttl(message);
        if ttl == 0 {
            return;
        }

        let wire = match message.to_vec() {
            Ok(v) => v,
            Err(_) => return,
        };

        let eb = entry_size(&key, wire.len());

        let entry = CacheEntry {
            message: wire,
            inserted_at: Instant::now(),
            ttl: Duration::from_secs(ttl as u64),
            _original_ttl: ttl,
            entry_bytes: eb,
        };

        let idx = Self::shard_index(&key);
        let mut shard = self.shards[idx].lock();

        // If this key already exists, reclaim its bytes first
        if let Some(old) = shard.lru.pop(&key) {
            shard.current_bytes -= old.entry_bytes;
        }

        shard.current_bytes += eb;

        // Evict LRU entries until we're within budget
        while shard.current_bytes > self.max_bytes_per_shard {
            if let Some((_evicted_key, evicted_entry)) = shard.lru.pop_lru() {
                shard.current_bytes -= evicted_entry.entry_bytes;
                self.stats.evictions.fetch_add(1, Ordering::Relaxed);
            } else {
                break;
            }
        }

        shard.lru.put(key, entry);
        self.stats.inserts.fetch_add(1, Ordering::Relaxed);
    }

    /// Get a cached DNS response, adjusting TTLs.
    /// Returns None if not found or expired.
    pub fn get(&self, key: &CacheKey) -> Option<Message> {
        let idx = Self::shard_index(key);
        let mut shard = self.shards[idx].lock();

        let entry = shard.lru.get(key)?;
        let elapsed = entry.inserted_at.elapsed();

        if elapsed >= entry.ttl {
            let eb = entry.entry_bytes;
            shard.lru.pop(key);
            shard.current_bytes -= eb;
            self.stats.misses.fetch_add(1, Ordering::Relaxed);
            return None;
        }

        let mut msg = match Message::from_vec(&entry.message) {
            Ok(m) => m,
            Err(_) => {
                let eb = entry.entry_bytes;
                shard.lru.pop(key);
                shard.current_bytes -= eb;
                self.stats.misses.fetch_add(1, Ordering::Relaxed);
                return None;
            }
        };

        // Adjust TTLs: subtract elapsed time
        let elapsed_secs = elapsed.as_secs() as u32;
        adjust_ttls(&mut msg, elapsed_secs);

        self.stats.hits.fetch_add(1, Ordering::Relaxed);
        Some(msg)
    }

    fn effective_ttl(&self, message: &Message) -> u32 {
        let mut min = u32::MAX;

        for record in message
            .answers()
            .iter()
            .chain(message.name_servers().iter())
            .chain(message.additionals().iter())
        {
            let ttl = record.ttl();
            if ttl < min {
                min = ttl;
            }
        }

        if min == u32::MAX {
            return 0;
        }

        min.clamp(self.min_ttl, self.max_ttl)
    }

    pub fn stats(&self) -> CacheStatsSnapshot {
        let hits = self.stats.hits.load(Ordering::Relaxed);
        let misses = self.stats.misses.load(Ordering::Relaxed);
        let total = hits + misses;
        let hit_rate = if total > 0 {
            hits as f64 / total as f64
        } else {
            0.0
        };

        let mut entries = 0usize;
        let mut bytes_used = 0usize;
        for s in &self.shards {
            let shard = s.lock();
            entries += shard.lru.len();
            bytes_used += shard.current_bytes;
        }

        CacheStatsSnapshot {
            hits,
            misses,
            inserts: self.stats.inserts.load(Ordering::Relaxed),
            evictions: self.stats.evictions.load(Ordering::Relaxed),
            hit_rate,
            entries,
            bytes_used,
        }
    }
}

fn adjust_ttls(msg: &mut Message, elapsed_secs: u32) {
    fn adjust_records(
        records: Vec<hickory_proto::rr::Record>,
        elapsed: u32,
    ) -> Vec<hickory_proto::rr::Record> {
        records
            .into_iter()
            .map(|mut r| {
                let ttl = r.ttl();
                r.set_ttl(ttl.saturating_sub(elapsed).max(1));
                r
            })
            .collect()
    }

    let answers = adjust_records(msg.take_answers(), elapsed_secs);
    let ns = adjust_records(msg.take_name_servers(), elapsed_secs);
    let additional = adjust_records(msg.take_additionals(), elapsed_secs);

    msg.insert_answers(answers);
    msg.insert_name_servers(ns);
    msg.insert_additionals(additional);
}

#[cfg(test)]
mod tests {
    use super::*;
    use hickory_proto::op::{Header, MessageType, OpCode, ResponseCode};
    use hickory_proto::rr::rdata::A;
    use hickory_proto::rr::{Name, RData, Record, RecordType};
    use std::net::Ipv4Addr;
    use std::str::FromStr;

    fn make_test_message(name: &str, ttl: u32) -> Message {
        let mut msg = Message::new();
        let mut header = Header::new();
        header.set_message_type(MessageType::Response);
        header.set_op_code(OpCode::Query);
        header.set_response_code(ResponseCode::NoError);
        msg.set_header(header);

        let record = Record::from_rdata(
            Name::from_str(name).unwrap(),
            ttl,
            RData::A(A(Ipv4Addr::new(1, 2, 3, 4))),
        );
        msg.add_answer(record);
        msg
    }

    #[test]
    fn test_cache_put_get() {
        let cache = DnsCache::new(1024 * 1024, 60, 86400); // 1MB
        let key = CacheKey {
            name: "example.com".to_string(),
            qtype: RecordType::A.into(),
            qclass: 1,
        };
        let msg = make_test_message("example.com.", 300);
        cache.put(key.clone(), &msg);

        let result = cache.get(&key);
        assert!(result.is_some());
        let cached_msg = result.unwrap();
        assert_eq!(cached_msg.answers().len(), 1);
    }

    #[test]
    fn test_cache_miss() {
        let cache = DnsCache::new(1024 * 1024, 60, 86400);
        let key = CacheKey {
            name: "nonexistent.com".to_string(),
            qtype: RecordType::A.into(),
            qclass: 1,
        };
        assert!(cache.get(&key).is_none());
    }

    #[test]
    fn test_cache_stats() {
        let cache = DnsCache::new(1024 * 1024, 60, 86400);
        let key = CacheKey {
            name: "test.com".to_string(),
            qtype: RecordType::A.into(),
            qclass: 1,
        };
        let msg = make_test_message("test.com.", 300);

        cache.put(key.clone(), &msg);
        cache.get(&key); // hit
        cache.get(&key); // hit

        let stats = cache.stats();
        assert_eq!(stats.hits, 2);
        assert_eq!(stats.inserts, 1);
        assert!(stats.bytes_used > 0);
    }

    #[test]
    fn test_min_ttl_enforcement() {
        let cache = DnsCache::new(1024 * 1024, 60, 86400);
        let key = CacheKey {
            name: "short-ttl.com".to_string(),
            qtype: RecordType::A.into(),
            qclass: 1,
        };
        // Create message with TTL of 5 seconds (below min_ttl of 60)
        let msg = make_test_message("short-ttl.com.", 5);
        cache.put(key.clone(), &msg);

        // Should still be cached because min_ttl is 60
        let result = cache.get(&key);
        assert!(result.is_some());
    }

    #[test]
    fn test_byte_eviction() {
        // Small cache: 32KB total. Each entry is ~170 bytes, so per-shard budget
        // (~128 bytes) means each shard holds at most 1 entry. Inserting 500 entries
        // across 256 shards forces evictions in shards that see multiple entries.
        let max_bytes = 32 * 1024;
        let cache = DnsCache::new(max_bytes, 60, 86400);

        for i in 0..500 {
            let key = CacheKey {
                name: format!("test{}.example.com", i),
                qtype: RecordType::A.into(),
                qclass: 1,
            };
            let msg = make_test_message(&format!("test{}.example.com.", i), 300);
            cache.put(key, &msg);
        }

        let stats = cache.stats();
        // With 500 inserts across 256 shards, many shards get 2+ entries
        // and must evict to stay within per-shard budget.
        assert!(stats.evictions > 0);
        // Total entries should be less than 500 (some were evicted)
        assert!(stats.entries < 500);
    }
}
