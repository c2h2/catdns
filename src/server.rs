use anyhow::Result;
use hickory_proto::op::Message;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, UdpSocket};
use tracing::{debug, info, warn};

use crate::handler::DnsHandler;

/// Start the UDP DNS server.
pub async fn run_udp_server(addr: &str, handler: Arc<DnsHandler>) -> Result<()> {
    let socket = Arc::new(UdpSocket::bind(addr).await?);
    info!("UDP DNS server listening on {}", addr);

    let mut buf = vec![0u8; 4096];
    loop {
        let (len, src) = match socket.recv_from(&mut buf).await {
            Ok(v) => v,
            Err(e) => {
                warn!("UDP recv error: {}", e);
                continue;
            }
        };

        let data = buf[..len].to_vec();
        let socket = socket.clone();
        let handler = handler.clone();

        tokio::spawn(async move {
            if let Err(e) = handle_udp_query(data, src, &socket, &handler).await {
                debug!("UDP query handler error from {}: {}", src, e);
            }
        });
    }
}

async fn handle_udp_query(
    data: Vec<u8>,
    src: SocketAddr,
    socket: &UdpSocket,
    handler: &DnsHandler,
) -> Result<()> {
    let query = Message::from_vec(&data)?;
    let response = handler.handle_query(query).await;
    let wire = response.to_vec()?;
    socket.send_to(&wire, src).await?;
    Ok(())
}

/// Start the TCP DNS server.
pub async fn run_tcp_server(addr: &str, handler: Arc<DnsHandler>) -> Result<()> {
    let listener = TcpListener::bind(addr).await?;
    info!("TCP DNS server listening on {}", addr);

    loop {
        let (stream, src) = match listener.accept().await {
            Ok(v) => v,
            Err(e) => {
                warn!("TCP accept error: {}", e);
                continue;
            }
        };

        let handler = handler.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_tcp_connection(stream, src, &handler).await {
                debug!("TCP connection error from {}: {}", src, e);
            }
        });
    }
}

async fn handle_tcp_connection(
    mut stream: tokio::net::TcpStream,
    _src: SocketAddr,
    handler: &DnsHandler,
) -> Result<()> {
    // Handle multiple queries on the same TCP connection
    loop {
        // Read 2-byte length prefix
        let mut len_buf = [0u8; 2];
        match stream.read_exact(&mut len_buf).await {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(e) => return Err(e.into()),
        }
        let msg_len = u16::from_be_bytes(len_buf) as usize;
        if msg_len == 0 || msg_len > 65535 {
            return Ok(());
        }

        let mut msg_buf = vec![0u8; msg_len];
        stream.read_exact(&mut msg_buf).await?;

        let query = Message::from_vec(&msg_buf)?;
        let response = handler.handle_query(query).await;
        let wire = response.to_vec()?;

        let len_bytes = (wire.len() as u16).to_be_bytes();
        stream.write_all(&len_bytes).await?;
        stream.write_all(&wire).await?;
    }
}
