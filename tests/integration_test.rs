use std::net::UdpSocket;
use std::process::{Child, Command};
use std::time::Duration;

struct TestServer {
    child: Child,
}

impl TestServer {
    fn start() -> Self {
        let child = Command::new("cargo")
            .args(["run", "--", "-c", "config.json"])
            .spawn()
            .expect("failed to start catdns");

        // Wait for server to start
        std::thread::sleep(Duration::from_secs(2));

        TestServer { child }
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn build_dns_query(name: &str, qtype: u16) -> Vec<u8> {
    let mut buf = Vec::new();

    // Header
    buf.extend_from_slice(&[0xAB, 0xCD]); // ID
    buf.extend_from_slice(&[0x01, 0x00]); // Flags: RD=1
    buf.extend_from_slice(&[0x00, 0x01]); // QDCOUNT=1
    buf.extend_from_slice(&[0x00, 0x00]); // ANCOUNT=0
    buf.extend_from_slice(&[0x00, 0x00]); // NSCOUNT=0
    buf.extend_from_slice(&[0x00, 0x00]); // ARCOUNT=0

    // Question: encode domain name
    for label in name.split('.') {
        buf.push(label.len() as u8);
        buf.extend_from_slice(label.as_bytes());
    }
    buf.push(0); // root label

    buf.extend_from_slice(&qtype.to_be_bytes()); // QTYPE
    buf.extend_from_slice(&[0x00, 0x01]); // QCLASS IN

    buf
}

fn parse_dns_response(buf: &[u8]) -> (u16, u16, u16) {
    // Returns (rcode, ancount, id)
    if buf.len() < 12 {
        return (0xFFFF, 0, 0);
    }
    let id = u16::from_be_bytes([buf[0], buf[1]]);
    let flags = u16::from_be_bytes([buf[2], buf[3]]);
    let rcode = flags & 0x000F;
    let ancount = u16::from_be_bytes([buf[6], buf[7]]);
    (rcode, ancount, id)
}

#[test]
fn test_gen_config() {
    let output = Command::new("cargo")
        .args(["run", "--", "--gen-config"])
        .output()
        .expect("failed to run gen-config");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("china_domains_file"));
    assert!(stdout.contains("china_upstreams"));
    assert!(stdout.contains("global_upstreams"));
    assert!(stdout.contains("cache"));
}

// NOTE: The following tests require network access and the server to be running.
// They are marked as #[ignore] by default.

#[test]
#[ignore]
fn test_china_domain_query() {
    let _server = TestServer::start();

    let socket = UdpSocket::bind("0.0.0.0:0").expect("bind failed");
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    socket.connect("127.0.0.1:10053").unwrap();

    // Query baidu.com A record (should go to China upstream)
    let query = build_dns_query("baidu.com", 1); // 1 = A
    socket.send(&query).unwrap();

    let mut buf = [0u8; 4096];
    let len = socket.recv(&mut buf).expect("recv failed");
    let (rcode, ancount, id) = parse_dns_response(&buf[..len]);

    assert_eq!(id, 0xABCD, "response ID should match query ID");
    assert_eq!(rcode, 0, "rcode should be NOERROR");
    assert!(ancount > 0, "should have answer records for baidu.com");
}

#[test]
#[ignore]
fn test_global_domain_query() {
    let _server = TestServer::start();

    let socket = UdpSocket::bind("0.0.0.0:0").expect("bind failed");
    socket
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    socket.connect("127.0.0.1:10053").unwrap();

    // Query google.com A record (should go to global upstream)
    let query = build_dns_query("google.com", 1);
    socket.send(&query).unwrap();

    let mut buf = [0u8; 4096];
    let len = socket.recv(&mut buf).expect("recv failed");
    let (rcode, ancount, id) = parse_dns_response(&buf[..len]);

    assert_eq!(id, 0xABCD);
    assert_eq!(rcode, 0, "rcode should be NOERROR");
    assert!(ancount > 0, "should have answer records for google.com");
}

#[test]
#[ignore]
fn test_cache_hit() {
    let _server = TestServer::start();

    let socket = UdpSocket::bind("0.0.0.0:0").expect("bind failed");
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    socket.connect("127.0.0.1:10053").unwrap();

    // Query twice - second should be cached
    let query = build_dns_query("baidu.com", 1);

    socket.send(&query).unwrap();
    let mut buf = [0u8; 4096];
    let _ = socket.recv(&mut buf).expect("first recv failed");

    // Second query
    socket.send(&query).unwrap();
    let len = socket.recv(&mut buf).expect("second recv failed");
    let (rcode, ancount, _) = parse_dns_response(&buf[..len]);
    assert_eq!(rcode, 0);
    assert!(ancount > 0);
}

#[test]
#[ignore]
fn test_api_stats() {
    let _server = TestServer::start();

    // First make a DNS query so there's some data
    let socket = UdpSocket::bind("0.0.0.0:0").expect("bind failed");
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    socket.connect("127.0.0.1:10053").unwrap();
    let query = build_dns_query("baidu.com", 1);
    socket.send(&query).unwrap();
    let mut buf = [0u8; 4096];
    let _ = socket.recv(&mut buf).ok();
    std::thread::sleep(Duration::from_millis(500));

    // Check API
    let resp = reqwest::blocking::get("http://127.0.0.1:8053/stats").expect("API request failed");
    assert!(resp.status().is_success());
    let body = resp.text().unwrap();
    assert!(body.contains("total_queries"));
    assert!(body.contains("cache"));
}
