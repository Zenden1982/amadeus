//! Приём соединений с заголовком PROXY protocol v2 (так Caddy L4 в standalone-профиле
//! передаёт настоящий IP клиента вместе с расшифрованным TCP-потоком к API).
//!
//! Заголовок читается в отдельной задаче на каждое соединение (с таймаутом), поэтому
//! медленный клиент не блокирует приём остальных.

use std::{io, net::SocketAddr, time::Duration};

use tokio::{
    io::{AsyncRead, AsyncReadExt},
    net::{TcpListener, TcpStream},
    sync::mpsc,
};

const SIGNATURE: [u8; 12] = *b"\r\n\r\n\0\r\nQUIT\n";
const MAX_LEN: usize = 4096;

/// Прочитать заголовок PROXY v2. `Ok(Some(addr))` — адрес клиента; `Ok(None)` — команда LOCAL
/// или не-IP семейство (использовать адрес пира). Ошибка — заголовка нет или он битый.
pub async fn read_v2_header<R: AsyncRead + Unpin>(r: &mut R) -> io::Result<Option<SocketAddr>> {
    let mut head = [0u8; 16];
    r.read_exact(&mut head).await?;
    if head[..12] != SIGNATURE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "нет заголовка PROXY v2",
        ));
    }
    let ver_cmd = head[12];
    if ver_cmd >> 4 != 2 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "PROXY: версия не 2",
        ));
    }
    let len = u16::from_be_bytes([head[14], head[15]]) as usize;
    if len > MAX_LEN {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "PROXY: слишком длинный заголовок",
        ));
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body).await?;
    if ver_cmd & 0x0f == 0 {
        return Ok(None); // LOCAL (health-check прокси)
    }
    let addr = match head[13] >> 4 {
        1 if len >= 12 => {
            let ip: [u8; 4] = body[0..4].try_into().expect("len checked");
            Some(SocketAddr::from((
                ip,
                u16::from_be_bytes([body[8], body[9]]),
            )))
        }
        2 if len >= 36 => {
            let ip: [u8; 16] = body[0..16].try_into().expect("len checked");
            Some(SocketAddr::from((
                ip,
                u16::from_be_bytes([body[32], body[33]]),
            )))
        }
        _ => None,
    };
    Ok(addr)
}

/// Слушатель для `axum::serve`, который снимает PROXY v2 с каждого соединения.
pub struct ProxyProtocolListener {
    rx: mpsc::Receiver<(TcpStream, SocketAddr)>,
    local: SocketAddr,
}

impl ProxyProtocolListener {
    pub fn new(listener: TcpListener, header_timeout: Duration) -> io::Result<Self> {
        let local = listener.local_addr()?;
        let (tx, rx) = mpsc::channel(64);
        tokio::spawn(async move {
            loop {
                let (mut stream, peer) = match listener.accept().await {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::warn!(error = %e, "accept failed");
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        continue;
                    }
                };
                let conn_tx = tx.clone();
                tokio::spawn(async move {
                    match tokio::time::timeout(header_timeout, read_v2_header(&mut stream)).await {
                        Ok(Ok(addr)) => {
                            let _ = conn_tx.send((stream, addr.unwrap_or(peer))).await;
                        }
                        Ok(Err(e)) => {
                            tracing::warn!(%peer, error = %e, "отброшено соединение без PROXY v2")
                        }
                        Err(_) => tracing::warn!(%peer, "таймаут заголовка PROXY v2"),
                    }
                });
                if tx.is_closed() {
                    break;
                }
            }
        });
        Ok(Self { rx, local })
    }
}

impl axum::serve::Listener for ProxyProtocolListener {
    type Io = TcpStream;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        match self.rx.recv().await {
            Some(v) => v,
            // приёмная задача завершилась — новых соединений не будет
            None => std::future::pending().await,
        }
    }

    fn local_addr(&self) -> io::Result<Self::Addr> {
        Ok(self.local)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::serve::Listener;
    use tokio::io::AsyncWriteExt;

    fn v2(cmd: u8, fam: u8, body: &[u8]) -> Vec<u8> {
        let mut v = SIGNATURE.to_vec();
        v.push(0x20 | cmd);
        v.push(fam);
        v.extend_from_slice(&(body.len() as u16).to_be_bytes());
        v.extend_from_slice(body);
        v
    }

    fn tcp4(src: [u8; 4], sport: u16) -> Vec<u8> {
        let mut b = src.to_vec();
        b.extend_from_slice(&[127, 0, 0, 1]);
        b.extend_from_slice(&sport.to_be_bytes());
        b.extend_from_slice(&443u16.to_be_bytes());
        b
    }

    #[tokio::test]
    async fn parses_v4_v6_local_and_rejects_garbage() {
        let h = v2(1, 0x11, &tcp4([203, 0, 113, 5], 5555));
        let got = read_v2_header(&mut h.as_slice()).await.unwrap();
        assert_eq!(got, Some("203.0.113.5:5555".parse().unwrap()));

        let mut b6 = vec![0u8; 36];
        b6[15] = 1; // ::1
        b6[32..34].copy_from_slice(&7000u16.to_be_bytes());
        let h = v2(1, 0x21, &b6);
        assert_eq!(
            read_v2_header(&mut h.as_slice()).await.unwrap(),
            Some("[::1]:7000".parse().unwrap())
        );

        let h = v2(0, 0x00, &[]);
        assert_eq!(read_v2_header(&mut h.as_slice()).await.unwrap(), None);

        let junk = b"GET / HTTP/1.1\r\nHost: x\r\n\r\n".to_vec();
        assert!(read_v2_header(&mut junk.as_slice()).await.is_err());
    }

    #[tokio::test]
    async fn listener_yields_real_client_addr_and_rest_of_stream() {
        let tcp = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut l = ProxyProtocolListener::new(tcp, Duration::from_secs(2)).unwrap();
        let addr = l.local_addr().unwrap();

        // соединение без заголовка отбрасывается и не мешает следующим
        let mut bad = TcpStream::connect(addr).await.unwrap();
        bad.write_all(b"GET / HTTP/1.1\r\n\r\n").await.unwrap();

        let mut c = TcpStream::connect(addr).await.unwrap();
        let mut msg = v2(1, 0x11, &tcp4([198, 51, 100, 7], 4242));
        msg.extend_from_slice(b"hello");
        c.write_all(&msg).await.unwrap();

        let (mut s, a) = tokio::time::timeout(Duration::from_secs(5), l.accept())
            .await
            .unwrap();
        assert_eq!(a, "198.51.100.7:4242".parse().unwrap());
        let mut buf = [0u8; 5];
        s.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"hello");
    }
}
