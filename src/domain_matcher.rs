use regex::Regex;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};

// ============================================================
// MixMatcher — mirrors mosdns pkg/matcher/domain/matcher.go
//
// Four sub-matchers, each activated by a prefix in the rule string:
//   "full:example.com"     → exact match (FullMatcher)
//   "domain:example.com"   → suffix/subdomain match (SubDomainMatcher, trie)
//   "keyword:google"       → substring match (KeywordMatcher)
//   "regexp:^ads?\."       → regex match (RegexMatcher)
//
// If no prefix is given, the default matcher is used ("domain").
// Match order: full → domain → keyword → regexp  (same as mosdns).
// ============================================================

/// Combined domain matcher with four strategies, same as mosdns MixMatcher.
pub struct DomainMatcher {
    full: FullMatcher,
    domain: SubDomainMatcher,
    keyword: KeywordMatcher,
    regex: RegexMatcher,
    default_matcher: MatcherType,
}

#[derive(Debug, Clone, Copy)]
enum MatcherType {
    Full,
    Domain,
    Keyword,
    Regexp,
}

impl DomainMatcher {
    pub fn new() -> Self {
        Self {
            full: FullMatcher::new(),
            domain: SubDomainMatcher::new(),
            keyword: KeywordMatcher::new(),
            regex: RegexMatcher::new(),
            default_matcher: MatcherType::Domain,
        }
    }

    /// Load domains from a reader (one rule per line).
    /// Lines starting with '#' or empty lines are skipped.
    /// Inline comments (text after '#') are stripped.
    /// Supports prefixes: "full:", "domain:", "keyword:", "regexp:"
    /// Default (no prefix) is suffix/subdomain match.
    pub fn load_from_reader<R: Read>(&mut self, reader: R) -> anyhow::Result<usize> {
        let buf = BufReader::new(reader);
        let mut loaded = 0;
        let mut line_num = 0;
        for line in buf.lines() {
            line_num += 1;
            let line = line?;
            // Strip inline comments
            let line = match line.find('#') {
                Some(pos) => &line[..pos],
                None => &line,
            };
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if let Err(e) = self.add(line) {
                return Err(anyhow::anyhow!("line {}: {}", line_num, e));
            }
            loaded += 1;
        }
        Ok(loaded)
    }

    /// Add a rule string. Parses prefix to determine matcher type.
    pub fn add(&mut self, rule: &str) -> anyhow::Result<()> {
        let (matcher_type, pattern) = self.split_type_and_pattern(rule);
        match matcher_type {
            MatcherType::Full => self.full.add(pattern),
            MatcherType::Domain => self.domain.add(pattern),
            MatcherType::Keyword => self.keyword.add(pattern),
            MatcherType::Regexp => self.regex.add(pattern),
        }
    }

    /// Match a domain against all sub-matchers.
    /// Order: full → domain → keyword → regexp (same as mosdns).
    pub fn matches(&self, domain: &str) -> bool {
        let normalized = normalize_domain(domain);
        self.full.matches(&normalized)
            || self.domain.matches(&normalized)
            || self.keyword.matches(&normalized)
            || self.regex.matches(&normalized)
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.full.len() + self.domain.len() + self.keyword.len() + self.regex.len()
    }

    fn split_type_and_pattern<'a>(&self, s: &'a str) -> (MatcherType, &'a str) {
        if let Some(p) = s.strip_prefix("full:") {
            (MatcherType::Full, p.trim())
        } else if let Some(p) = s.strip_prefix("domain:") {
            (MatcherType::Domain, p.trim())
        } else if let Some(p) = s.strip_prefix("keyword:") {
            (MatcherType::Keyword, p.trim())
        } else if let Some(p) = s.strip_prefix("regexp:") {
            (MatcherType::Regexp, p.trim())
        } else {
            (self.default_matcher, s)
        }
    }
}

// ============================================================
// FullMatcher — exact domain match (hash map)
// ============================================================

struct FullMatcher {
    m: HashMap<String, ()>,
}

impl FullMatcher {
    fn new() -> Self {
        Self { m: HashMap::new() }
    }

    fn add(&mut self, pattern: &str) -> anyhow::Result<()> {
        let normalized = normalize_domain(pattern);
        self.m.insert(normalized, ());
        Ok(())
    }

    fn matches(&self, normalized: &str) -> bool {
        self.m.contains_key(normalized)
    }

    fn len(&self) -> usize {
        self.m.len()
    }
}

// ============================================================
// SubDomainMatcher — trie-based suffix/subdomain match
// Mirrors mosdns SubDomainMatcher with ReverseDomainScanner.
// ============================================================

struct SubDomainMatcher {
    root: TrieNode,
    count: usize,
}

struct TrieNode {
    children: HashMap<Box<str>, TrieNode>,
    is_terminal: bool,
}

impl TrieNode {
    fn new() -> Self {
        Self {
            children: HashMap::new(),
            is_terminal: false,
        }
    }
}

impl SubDomainMatcher {
    fn new() -> Self {
        Self {
            root: TrieNode::new(),
            count: 0,
        }
    }

    fn add(&mut self, pattern: &str) -> anyhow::Result<()> {
        let normalized = normalize_domain(pattern);
        // Walk labels in reverse (same as mosdns ReverseDomainScanner)
        let mut node = &mut self.root;
        for label in normalized.rsplit('.') {
            node = node
                .children
                .entry(label.into())
                .or_insert_with(TrieNode::new);
        }
        if !node.is_terminal {
            node.is_terminal = true;
            self.count += 1;
        }
        Ok(())
    }

    /// Suffix match: "www.baidu.com" matches if "baidu.com" is in the trie.
    /// Walks labels in reverse and returns true if any node along the path
    /// is terminal (matching mosdns behavior).
    fn matches(&self, normalized: &str) -> bool {
        let mut node = &self.root;
        // Check if root itself is terminal (matches everything)
        if node.is_terminal {
            return true;
        }
        for label in normalized.rsplit('.') {
            match node.children.get(label) {
                Some(child) => {
                    node = child;
                    if node.is_terminal {
                        return true;
                    }
                }
                None => return false,
            }
        }
        false
    }

    fn len(&self) -> usize {
        self.count
    }
}

// ============================================================
// KeywordMatcher — substring match
// ============================================================

struct KeywordMatcher {
    keywords: Vec<String>,
}

impl KeywordMatcher {
    fn new() -> Self {
        Self {
            keywords: Vec::new(),
        }
    }

    fn add(&mut self, pattern: &str) -> anyhow::Result<()> {
        let normalized = normalize_domain(pattern);
        self.keywords.push(normalized);
        Ok(())
    }

    fn matches(&self, normalized: &str) -> bool {
        self.keywords.iter().any(|kw| normalized.contains(kw.as_str()))
    }

    fn len(&self) -> usize {
        self.keywords.len()
    }
}

// ============================================================
// RegexMatcher — compiled regex match
// ============================================================

struct RegexMatcher {
    patterns: Vec<Regex>,
}

impl RegexMatcher {
    fn new() -> Self {
        Self {
            patterns: Vec::new(),
        }
    }

    fn add(&mut self, pattern: &str) -> anyhow::Result<()> {
        let re = Regex::new(pattern)
            .map_err(|e| anyhow::anyhow!("invalid regexp '{}': {}", pattern, e))?;
        self.patterns.push(re);
        Ok(())
    }

    fn matches(&self, normalized: &str) -> bool {
        self.patterns.iter().any(|re| re.is_match(normalized))
    }

    fn len(&self) -> usize {
        self.patterns.len()
    }
}

// ============================================================
// Helpers
// ============================================================

fn normalize_domain(domain: &str) -> String {
    domain.to_ascii_lowercase().trim_end_matches('.').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- SubDomainMatcher (default, same as "domain:" prefix) ---

    #[test]
    fn test_basic_suffix_matching() {
        let mut m = DomainMatcher::new();
        m.add("baidu.com").unwrap();
        m.add("qq.com").unwrap();
        m.add("cn").unwrap();

        assert!(m.matches("baidu.com"));
        assert!(m.matches("www.baidu.com"));
        assert!(m.matches("tieba.baidu.com"));
        assert!(m.matches("qq.com"));
        assert!(m.matches("mail.qq.com"));
        assert!(m.matches("test.cn"));
        assert!(m.matches("a.b.c.cn"));

        assert!(!m.matches("google.com"));
        assert!(!m.matches("notbaidu.com"));
        assert!(!m.matches("com"));
    }

    #[test]
    fn test_domain_prefix() {
        let mut m = DomainMatcher::new();
        m.add("domain:example.com").unwrap();
        assert!(m.matches("example.com"));
        assert!(m.matches("www.example.com"));
        assert!(!m.matches("notexample.com"));
    }

    // --- FullMatcher (exact match) ---

    #[test]
    fn test_full_match() {
        let mut m = DomainMatcher::new();
        m.add("full:example.com").unwrap();
        assert!(m.matches("example.com"));
        assert!(m.matches("EXAMPLE.COM"));   // case insensitive
        assert!(m.matches("example.com."));  // fqdn insensitive
        assert!(!m.matches("www.example.com")); // not a suffix match
        assert!(!m.matches("sub.example.com"));
    }

    // --- KeywordMatcher (substring match) ---

    #[test]
    fn test_keyword_match() {
        let mut m = DomainMatcher::new();
        m.add("keyword:google").unwrap();
        assert!(m.matches("google.com"));
        assert!(m.matches("www.google.com"));
        assert!(m.matches("mail.google.co.jp"));
        assert!(m.matches("ads.google.analytics.com"));
        assert!(!m.matches("example.com"));
    }

    // --- RegexMatcher ---

    #[test]
    fn test_regexp_match() {
        let mut m = DomainMatcher::new();
        m.add(r"regexp:^ad[sx]?\..+").unwrap();
        assert!(m.matches("ad.example.com"));
        assert!(m.matches("ads.example.com"));
        assert!(m.matches("adx.tracker.net"));
        assert!(!m.matches("example.com"));
        assert!(!m.matches("bad.example.com"));
    }

    // --- MixMatcher match order (full > domain > keyword > regexp) ---

    #[test]
    fn test_mix_matcher_all_types() {
        let mut m = DomainMatcher::new();
        m.add("full:exact.example.com").unwrap();
        m.add("domain:suffix.com").unwrap();
        m.add("keyword:track").unwrap();
        m.add(r"regexp:^log\d+\.").unwrap();

        // full
        assert!(m.matches("exact.example.com"));
        assert!(!m.matches("www.exact.example.com"));

        // domain (suffix)
        assert!(m.matches("suffix.com"));
        assert!(m.matches("www.suffix.com"));

        // keyword
        assert!(m.matches("tracker.example.com"));
        assert!(m.matches("www.tracking.net"));

        // regexp
        assert!(m.matches("log1.example.com"));
        assert!(m.matches("log42.data.net"));
        assert!(!m.matches("blog1.example.com"));
    }

    // --- File loading ---

    #[test]
    fn test_load_from_reader() {
        let data = b"# China domains\nbaidu.com\ndomain:qq.com\nfull:taobao.com\nkeyword:tencent\n\n";
        let mut m = DomainMatcher::new();
        let count = m.load_from_reader(&data[..]).unwrap();
        assert_eq!(count, 4);
        assert!(m.matches("www.baidu.com"));     // default → domain
        assert!(m.matches("www.qq.com"));        // domain:
        assert!(m.matches("taobao.com"));        // full:
        assert!(!m.matches("www.taobao.com"));   // full: no suffix
        assert!(m.matches("tencent.com"));       // keyword:
        assert!(m.matches("img.tencent.cn"));    // keyword:
    }

    #[test]
    fn test_load_inline_comments() {
        let data = b"baidu.com # main site\nqq.com\n# full comment line\n";
        let mut m = DomainMatcher::new();
        let count = m.load_from_reader(&data[..]).unwrap();
        assert_eq!(count, 2);
        assert!(m.matches("baidu.com"));
        assert!(m.matches("qq.com"));
    }

    #[test]
    fn test_case_insensitive() {
        let mut m = DomainMatcher::new();
        m.add("Baidu.COM").unwrap();
        assert!(m.matches("BAIDU.com"));
        assert!(m.matches("www.baidu.com"));
    }

    #[test]
    fn test_trailing_dot() {
        let mut m = DomainMatcher::new();
        m.add("baidu.com.").unwrap();
        assert!(m.matches("baidu.com"));
        assert!(m.matches("www.baidu.com."));
    }

    // --- mosdns dnsmasq-china-list format ---

    #[test]
    fn test_dnsmasq_format_parsing() {
        // mosdns's update script parses dnsmasq server=/ lines into plain domains.
        // Our loader handles the plain domain format that results from that.
        let data = b"163.com\nbaidu.com\nbilibili.com\ncn\n";
        let mut m = DomainMatcher::new();
        let count = m.load_from_reader(&data[..]).unwrap();
        assert_eq!(count, 4);
        assert!(m.matches("www.163.com"));
        assert!(m.matches("tieba.baidu.com"));
        assert!(m.matches("live.bilibili.com"));
        assert!(m.matches("tsinghua.edu.cn"));
        assert!(!m.matches("google.com"));
    }

    #[test]
    fn test_regexp_error() {
        let mut m = DomainMatcher::new();
        assert!(m.add(r"regexp:[invalid").is_err());
    }
}
