mod api;
mod cache;
mod config;
mod domain_matcher;
mod handler;
mod history;
mod server;
mod upstream;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info};

#[derive(Parser)]
#[command(name = "catdns", about = "A fast DNS forwarder with China domain routing")]
struct Cli {
    /// Path to config file (JSON)
    #[arg(short, long, default_value = "config.json")]
    config: PathBuf,

    /// Generate example config and exit
    #[arg(long)]
    gen_config: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.gen_config {
        let example = config::Config::example();
        let json = serde_json::to_string_pretty(&example)?;
        println!("{}", json);
        return Ok(());
    }

    // Load config
    let cfg = config::Config::load(&cli.config)?;

    // Init logging
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&cfg.log_level));
    tracing_subscriber::fmt().with_env_filter(env_filter).init();

    info!("catdns starting...");

    // Load China domain matcher
    let mut matcher = domain_matcher::DomainMatcher::new();
    match std::fs::File::open(&cfg.china_domains_file) {
        Ok(file) => {
            let count = matcher.load_from_reader(file)?;
            info!("loaded {} China domains from {}", count, cfg.china_domains_file);
        }
        Err(e) => {
            error!(
                "failed to open China domains file '{}': {}",
                cfg.china_domains_file, e
            );
            error!("continuing without China domain matching - all queries go to global upstreams");
        }
    }
    let matcher = Arc::new(matcher);

    // Init upstreams
    let china_upstream = Arc::new(upstream::UpstreamGroup::new("china", &cfg.china_upstreams)?);
    let global_upstream = Arc::new(upstream::UpstreamGroup::new("global", &cfg.global_upstreams)?);
    info!(
        "upstreams: {} china, {} global",
        cfg.china_upstreams.len(),
        cfg.global_upstreams.len()
    );

    // Init cache
    let dns_cache = Arc::new(cache::DnsCache::new(
        cfg.cache.max_bytes,
        cfg.cache.min_ttl,
        cfg.cache.max_ttl,
    ));
    info!("cache: max {}MB", cfg.cache.max_bytes / (1024 * 1024));

    // Init history
    let query_history = Arc::new(history::QueryHistory::new());

    // Init handler
    let dns_handler = Arc::new(handler::DnsHandler::new(
        matcher,
        china_upstream.clone(),
        global_upstream.clone(),
        dns_cache.clone(),
        query_history.clone(),
        cfg.prefer_v4,
        Duration::from_millis(cfg.query_timeout_ms),
    ));

    // API state
    let api_state = Arc::new(api::AppState {
        handler: dns_handler.clone(),
        cache: dns_cache,
        history: query_history,
        china_upstream,
        global_upstream,
        start_time: std::time::Instant::now(),
    });

    // Start servers
    let listen = cfg.listen.clone();
    let listen_proto = cfg.listen_proto.clone();

    let mut tasks = Vec::new();

    // HTTP API
    let api_addr = cfg.api_listen.clone();
    tasks.push(tokio::spawn(async move {
        if let Err(e) = api::run_api_server(&api_addr, api_state).await {
            error!("API server error: {}", e);
        }
    }));

    // DNS servers
    match listen_proto.as_str() {
        "udp" => {
            let h = dns_handler.clone();
            let a = listen.clone();
            tasks.push(tokio::spawn(async move {
                if let Err(e) = server::run_udp_server(&a, h).await {
                    error!("UDP server error: {}", e);
                }
            }));
        }
        "tcp" => {
            let h = dns_handler.clone();
            let a = listen.clone();
            tasks.push(tokio::spawn(async move {
                if let Err(e) = server::run_tcp_server(&a, h).await {
                    error!("TCP server error: {}", e);
                }
            }));
        }
        "both" | _ => {
            let h1 = dns_handler.clone();
            let a1 = listen.clone();
            tasks.push(tokio::spawn(async move {
                if let Err(e) = server::run_udp_server(&a1, h1).await {
                    error!("UDP server error: {}", e);
                }
            }));

            let h2 = dns_handler.clone();
            let a2 = listen.clone();
            tasks.push(tokio::spawn(async move {
                if let Err(e) = server::run_tcp_server(&a2, h2).await {
                    error!("TCP server error: {}", e);
                }
            }));
        }
    }

    info!("catdns is running. DNS={} API={}", listen, cfg.api_listen);

    // Wait for any task to finish (shouldn't happen normally)
    for task in tasks {
        task.await?;
    }

    Ok(())
}
