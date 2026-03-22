use hickory_proto::rr::RecordType;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::time::Duration;

const MAX_HISTORY: usize = 1000;

pub struct QueryHistory {
    entries: Mutex<VecDeque<QueryRecord>>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct QueryRecord {
    pub timestamp: String,
    pub qname: String,
    pub qtype: String,
    pub cached: bool,
    pub china: bool,
    pub elapsed_ms: f64,
}

impl QueryHistory {
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(VecDeque::with_capacity(MAX_HISTORY)),
        }
    }

    pub fn record(
        &self,
        qname: &str,
        qtype: RecordType,
        cached: bool,
        china: bool,
        elapsed: Duration,
    ) {
        let record = QueryRecord {
            timestamp: chrono::Utc::now().to_rfc3339(),
            qname: qname.to_string(),
            qtype: format!("{:?}", qtype),
            cached,
            china,
            elapsed_ms: elapsed.as_secs_f64() * 1000.0,
        };

        let mut entries = self.entries.lock();
        if entries.len() >= MAX_HISTORY {
            entries.pop_front();
        }
        entries.push_back(record);
    }

    pub fn recent(&self, limit: usize) -> Vec<QueryRecord> {
        let entries = self.entries.lock();
        let limit = limit.min(entries.len());
        entries.iter().rev().take(limit).cloned().collect()
    }
}
