use hickory_proto::op::{Header, Message, MessageType, OpCode, ResponseCode};
use hickory_proto::rr::RecordType;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, warn};

use crate::cache::{CacheKey, DnsCache};
use crate::domain_matcher::DomainMatcher;
use crate::history::QueryHistory;
use crate::upstream::UpstreamGroup;

pub struct DnsHandler {
    china_matcher: Arc<DomainMatcher>,
    china_upstream: Arc<UpstreamGroup>,
    global_upstream: Arc<UpstreamGroup>,
    cache: Arc<DnsCache>,
    history: Arc<QueryHistory>,
    prefer_v4: bool,
    query_timeout: Duration,

    // Stats
    total_queries: AtomicU64,
    china_queries: AtomicU64,
    global_queries: AtomicU64,
}

impl DnsHandler {
    pub fn new(
        china_matcher: Arc<DomainMatcher>,
        china_upstream: Arc<UpstreamGroup>,
        global_upstream: Arc<UpstreamGroup>,
        cache: Arc<DnsCache>,
        history: Arc<QueryHistory>,
        prefer_v4: bool,
        query_timeout: Duration,
    ) -> Self {
        Self {
            china_matcher,
            china_upstream,
            global_upstream,
            cache,
            history,
            prefer_v4,
            query_timeout,
            total_queries: AtomicU64::new(0),
            china_queries: AtomicU64::new(0),
            global_queries: AtomicU64::new(0),
        }
    }

    /// Handle a DNS query message, return response message.
    pub async fn handle_query(&self, query: Message) -> Message {
        let start = Instant::now();
        self.total_queries.fetch_add(1, Ordering::Relaxed);

        // Validate query
        if query.query_count() == 0 {
            return Self::make_error_response(&query, ResponseCode::FormErr);
        }

        let q = query.queries()[0].clone();
        let qname = q.name().to_string();
        let qtype = q.query_type();

        // prefer_v4: if querying AAAA, also check if we should suppress it
        // We handle this after getting the response.

        // Check cache
        let cache_key = CacheKey {
            name: qname.clone(),
            qtype: qtype.into(),
            qclass: q.query_class().into(),
        };

        if let Some(cached) = self.cache.get(&cache_key) {
            debug!(qname = %qname, qtype = ?qtype, "cache hit");
            let mut resp = cached;
            resp.set_id(query.id());
            self.history.record(
                &qname,
                qtype,
                true,
                self.is_china_domain(&qname),
                start.elapsed(),
            );
            return resp;
        }

        // Determine upstream group
        let is_china = self.is_china_domain(&qname);
        let (upstream, group_name) = if is_china {
            self.china_queries.fetch_add(1, Ordering::Relaxed);
            (&self.china_upstream, "china")
        } else {
            self.global_queries.fetch_add(1, Ordering::Relaxed);
            (&self.global_upstream, "global")
        };

        debug!(qname = %qname, qtype = ?qtype, group = %group_name, "forwarding query");

        // Forward to upstream
        match upstream.exchange(&query, self.query_timeout).await {
            Ok(mut response) => {
                // prefer_v4: if we got an AAAA response, filter it out if A records exist
                if self.prefer_v4 && qtype == RecordType::AAAA {
                    if self.has_a_record_cached(&qname, q.query_class().into()) {
                        // Return empty AAAA response to encourage fallback to A
                        let mut empty = Message::new();
                        empty.set_id(query.id());
                        let mut header = Header::new();
                        header.set_message_type(MessageType::Response);
                        header.set_op_code(OpCode::Query);
                        header.set_response_code(ResponseCode::NoError);
                        header.set_recursion_desired(true);
                        header.set_recursion_available(true);
                        empty.set_header(header);
                        empty.add_query(q);
                        self.history.record(
                            &qname,
                            qtype,
                            false,
                            is_china,
                            start.elapsed(),
                        );
                        return empty;
                    }
                }

                // Cache the response
                response.set_id(query.id());
                self.cache.put(cache_key, &response);

                self.history
                    .record(&qname, qtype, false, is_china, start.elapsed());
                response
            }
            Err(e) => {
                warn!(
                    qname = %qname,
                    qtype = ?qtype,
                    group = %group_name,
                    "upstream failed: {}",
                    e
                );
                self.history
                    .record(&qname, qtype, false, is_china, start.elapsed());
                Self::make_error_response(&query, ResponseCode::ServFail)
            }
        }
    }

    fn is_china_domain(&self, qname: &str) -> bool {
        self.china_matcher.matches(qname)
    }

    fn has_a_record_cached(&self, name: &str, qclass: u16) -> bool {
        let key = CacheKey {
            name: name.to_string(),
            qtype: RecordType::A.into(),
            qclass,
        };
        self.cache.get(&key).is_some()
    }

    fn make_error_response(query: &Message, rcode: ResponseCode) -> Message {
        let mut resp = Message::new();
        resp.set_id(query.id());
        let mut header = Header::new();
        header.set_message_type(MessageType::Response);
        header.set_op_code(OpCode::Query);
        header.set_response_code(rcode);
        header.set_recursion_desired(true);
        header.set_recursion_available(true);
        resp.set_header(header);
        for q in query.queries() {
            resp.add_query(q.clone());
        }
        resp
    }

    pub fn stats(&self) -> HandlerStats {
        HandlerStats {
            total_queries: self.total_queries.load(Ordering::Relaxed),
            china_queries: self.china_queries.load(Ordering::Relaxed),
            global_queries: self.global_queries.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct HandlerStats {
    pub total_queries: u64,
    pub china_queries: u64,
    pub global_queries: u64,
}
