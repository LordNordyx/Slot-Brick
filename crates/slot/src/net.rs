//! A small HTTP/1.0 client over std's TcpStream, with TLS from rustls (ring, Mozilla's
//! roots built in, so nothing depends on the firmware's certificate store). HTTP/1.0
//! keeps bodies unchunked; one request per connection.
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

/// What every request says it is.
pub fn user_agent() -> String {
    format!(
        "SlotBrick/{} (Linux {}) rcheevos/{}",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::ARCH,
        slot_cheevos::version()
    )
}

pub struct Request<'a> {
    pub tls: bool,
    /// host:port
    pub server: &'a str,
    pub method: &'a str,
    pub path: &'a str,
    /// An application/x-www-form-urlencoded body, for POST.
    pub form: Option<&'a str>,
    /// The most body accepted.
    pub limit: usize,
}

pub struct Response {
    pub status: u16,
    head: String,
    pub body: Vec<u8>,
}

impl Response {
    pub fn header(&self, name: &str) -> Option<String> {
        self.head.lines().skip(1).find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.trim().eq_ignore_ascii_case(name).then(|| v.trim().to_string())
        })
    }
}

pub fn host_of(server: &str) -> &str {
    server.rsplit_once(':').map_or(server, |(h, _)| h)
}

/// `https://host[:port]/path` (or `http://`) as (tls, "host:port", "/path").
pub fn split_url(url: &str) -> Option<(bool, String, String)> {
    let (tls, rest) = match (url.strip_prefix("https://"), url.strip_prefix("http://")) {
        (Some(rest), _) => (true, rest),
        (None, Some(rest)) => (false, rest),
        _ => return None,
    };
    let (host, path) = rest.split_once('/').map_or((rest, "/".to_string()), |(h, p)| (h, format!("/{p}")));
    if host.is_empty() {
        return None;
    }
    let server = match host.contains(':') {
        true => host.to_string(),
        false => format!("{host}:{}", if tls { 443 } else { 80 }),
    };
    Some((tls, server, path))
}

/// Request-line parts come from servers (redirects, badge URLs): a space or control
/// character in one would let it add headers or a second request.
fn plain(part: &str) -> bool {
    !part.is_empty() && part.bytes().all(|b| b.is_ascii_graphic())
}

pub fn send(req: &Request) -> Result<Response, String> {
    let host = host_of(req.server);
    if !(plain(req.method) && plain(req.server) && plain(req.path) && req.path.starts_with('/')) {
        return Err("bad request address".into());
    }
    let tcp = dial(req.server)?;
    let mut text = format!(
        "{} {} HTTP/1.0\r\nHost: {host}\r\nUser-Agent: {}\r\nConnection: close\r\n",
        req.method,
        req.path,
        user_agent()
    );
    if let Some(form) = req.form {
        text.push_str(&format!(
            "Content-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{form}",
            form.len()
        ));
    } else {
        text.push_str("\r\n");
    }
    let cap = req.limit as u64 + 64 * 1024;
    let raw = if req.tls {
        let name = rustls::pki_types::ServerName::try_from(host.to_string())
            .map_err(|_| format!("bad host name {host}"))?;
        let conn = rustls::ClientConnection::new(tls_config(), name).map_err(|e| e.to_string())?;
        let mut stream = rustls::StreamOwned::new(conn, tcp);
        // One write: a request split over several packets can be answered and reset early.
        stream.write_all(text.as_bytes()).map_err(|e| tls_error(&e))?;
        read_all(stream, cap).map_err(|e| tls_error(&e))?
    } else {
        let mut tcp = tcp;
        tcp.write_all(text.as_bytes()).map_err(|e| format!("send failed: {e}"))?;
        read_all(tcp, cap).map_err(|e| format!("download failed: {e}"))?
    };
    parse(&raw, req.limit)
}

fn dial(server: &str) -> Result<TcpStream, String> {
    let host = host_of(server);
    let addrs: Vec<_> = server
        .to_socket_addrs()
        .map_err(|_| format!("cannot look up {host} (no internet?)"))?
        .collect();
    let tcp = addrs
        .iter()
        .find_map(|a| TcpStream::connect_timeout(a, Duration::from_secs(8)).ok())
        .ok_or_else(|| format!("cannot connect to {host}"))?;
    let _ = tcp.set_read_timeout(Some(Duration::from_secs(20)));
    let _ = tcp.set_write_timeout(Some(Duration::from_secs(8)));
    Ok(tcp)
}

/// A body being downloaded: its length, if the server said, and the bytes as they come.
pub struct Body {
    pub len: Option<u64>,
    pub reader: Box<dyn Read + Send>,
}

/// GET `url`, following redirects (a GitHub download is one, to its file server), with the
/// body left to be read as it arrives rather than held in memory. Only a 200 is a body.
pub fn get_stream(url: &str) -> Result<Body, String> {
    let mut url = url.to_string();
    for _ in 0..5 {
        let (tls, server, path) = split_url(&url).ok_or("bad download address")?;
        let (status, head, reader) = open_get(tls, &server, &path)?;
        let reply = Response { status, head, body: Vec::new() };
        match status {
            200 => {
                let len = reply.header("content-length").and_then(|v| v.parse().ok());
                return Ok(Body { len, reader });
            }
            301 | 302 | 303 | 307 | 308 => {
                let to = reply.header("location").ok_or("bad redirect from server")?;
                url = match to.starts_with('/') {
                    true => format!("{}://{server}{to}", if tls { "https" } else { "http" }),
                    false => to,
                };
            }
            _ => return Err(format!("server said {status}")),
        }
    }
    Err("too many redirects".into())
}

/// GET `url` whole, following redirects; at most `limit` bytes.
pub fn get(url: &str, limit: usize) -> Result<Vec<u8>, String> {
    let body = get_stream(url)?;
    if body.len.is_some_and(|n| n > limit as u64) {
        return Err("reply too large".into());
    }
    let mut out = Vec::new();
    match body.reader.take(limit as u64 + 1).read_to_end(&mut out) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof && !out.is_empty() => {}
        Err(e) => return Err(format!("download failed: {e}")),
    }
    if out.len() > limit {
        return Err("reply too large".into());
    }
    if body.len.is_some_and(|n| (out.len() as u64) < n) {
        return Err("download cut short".into());
    }
    Ok(out)
}

/// Sends a GET and reads up to the end of its headers: the status, the headers, and the
/// connection, at the body's first byte.
fn open_get(tls: bool, server: &str, path: &str) -> Result<(u16, String, Box<dyn Read + Send>), String> {
    let host = host_of(server);
    if !(plain(server) && plain(path) && path.starts_with('/')) {
        return Err("bad request address".into());
    }
    let tcp = dial(server)?;
    let text = format!(
        "GET {path} HTTP/1.0\r\nHost: {host}\r\nUser-Agent: {}\r\nConnection: close\r\n\r\n",
        user_agent()
    );
    let mut stream: Box<dyn Read + Send> = if tls {
        let name = rustls::pki_types::ServerName::try_from(host.to_string())
            .map_err(|_| format!("bad host name {host}"))?;
        let conn = rustls::ClientConnection::new(tls_config(), name).map_err(|e| e.to_string())?;
        let mut stream = rustls::StreamOwned::new(conn, tcp);
        stream.write_all(text.as_bytes()).map_err(|e| tls_error(&e))?;
        Box::new(stream)
    } else {
        let mut tcp = tcp;
        tcp.write_all(text.as_bytes()).map_err(|e| format!("send failed: {e}"))?;
        Box::new(tcp)
    };
    // A byte at a time, so nothing of the body is read here: the caller reads all of it.
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if head.len() > 64 * 1024 {
            return Err("bad reply from server".into());
        }
        match stream.read(&mut byte) {
            Ok(1) => head.push(byte[0]),
            Ok(_) => return Err("bad reply from server".into()),
            Err(e) if tls => return Err(tls_error(&e)),
            Err(e) => return Err(format!("download failed: {e}")),
        }
    }
    let head = String::from_utf8_lossy(&head[..head.len() - 4]).into_owned();
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or("bad reply from server")?;
    Ok((status, head, stream))
}

/// To the end of the stream. A server that closes TLS without a close_notify
/// (common) ends the read with UnexpectedEof; the reply is complete by then.
fn read_all(stream: impl Read, cap: u64) -> std::io::Result<Vec<u8>> {
    let mut raw = Vec::new();
    match stream.take(cap).read_to_end(&mut raw) {
        Ok(_) => Ok(raw),
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof && !raw.is_empty() => Ok(raw),
        Err(e) => Err(e),
    }
}

/// `send`'s error for a server it could not reach at all (no internet, wrong network), as
/// opposed to one that answered badly: worth giving up a whole run over.
pub fn unreachable(error: &str) -> bool {
    error.starts_with("cannot look up ") || error.starts_with("cannot connect to ")
}

fn tls_error(e: &std::io::Error) -> String {
    let text = e.to_string();
    let lower = text.to_lowercase();
    // A certificate checked against a wrong clock: the Brick's clock is the usual cause.
    if lower.contains("notvalidyet") || lower.contains("not valid yet") || lower.contains("expired") {
        return "secure connection refused: check Date & Time in settings".into();
    }
    format!("secure connection failed: {text}")
}

fn tls_config() -> Arc<rustls::ClientConfig> {
    static CONFIG: OnceLock<Arc<rustls::ClientConfig>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            let roots = rustls::RootCertStore {
                roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
            };
            let provider = Arc::new(rustls::crypto::ring::default_provider());
            let config = rustls::ClientConfig::builder_with_provider(provider)
                .with_safe_default_protocol_versions()
                .expect("ring supports the default TLS versions")
                .with_root_certificates(roots)
                .with_no_client_auth();
            Arc::new(config)
        })
        .clone()
}

/// Status line, headers and body. A body shorter than its Content-Length is an error.
pub fn parse(raw: &[u8], limit: usize) -> Result<Response, String> {
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or("bad reply from server")?;
    let head = String::from_utf8_lossy(&raw[..split]).into_owned();
    let body = raw[split + 4..].to_vec();
    let status: u16 = head
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or("bad reply from server")?;
    let reply = Response { status, head, body };
    if reply.body.len() > limit {
        return Err("reply too large".into());
    }
    if let Some(len) = reply.header("content-length").and_then(|v| v.parse::<usize>().ok()) {
        if reply.body.len() < len {
            return Err("download cut short".into());
        }
    }
    Ok(reply)
}

/// application/x-www-form-urlencoded value.
pub fn form_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3 / 2);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_form_value_escapes_everything_but_unreserved() {
        assert_eq!(form_escape("p@ss word&x=1"), "p%40ss%20word%26x%3D1");
    }

    #[test]
    fn a_reply_shorter_than_its_length_is_refused() {
        assert!(parse(b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\nabc", 100).is_err());
        let ok = parse(b"HTTP/1.1 401 No\r\nContent-Length: 3\r\nX-A: b\r\n\r\nabc", 100).unwrap();
        assert_eq!((ok.status, ok.body.as_slice(), ok.header("x-a").as_deref()), (401, &b"abc"[..], Some("b")));
    }

    #[test]
    fn urls_split_into_server_and_path() {
        assert_eq!(split_url("https://a.org/x?y=1"), Some((true, "a.org:443".into(), "/x?y=1".into())));
        assert_eq!(split_url("http://h:8080"), Some((false, "h:8080".into(), "/".into())));
        assert_eq!(split_url("ftp://h/x"), None);
        assert_eq!(split_url("https:///x"), None);
    }

    #[test]
    fn a_request_line_cannot_carry_extra_headers() {
        for (server, path) in [("h:80", "/a\r\nX: y"), ("h:80", "/a b"), ("h\r\n:80", "/"), ("h:80", "a")] {
            let req = Request { tls: false, server, method: "GET", path, form: None, limit: 1 };
            assert_eq!(send(&req).err().as_deref(), Some("bad request address"), "{path:?}");
        }
    }

    #[test]
    fn only_an_unreachable_server_counts_as_unreachable() {
        let req = |server| Request { tls: false, server, method: "GET", path: "/", form: None, limit: 1 };
        assert!(unreachable(&send(&req("127.0.0.1:1")).err().unwrap()));
        assert!(unreachable(&send(&req("no-such-host.invalid:80")).err().unwrap()));
        assert!(!unreachable("server said 403") && !unreachable("reply too large"));
    }

    #[test]
    fn a_wrong_clock_is_named_as_the_cause() {
        let e = std::io::Error::other("invalid peer certificate: NotValidYet");
        assert!(tls_error(&e).contains("Date & Time"));
    }

    /// `SLOT_LIVE_NET=1 ... live_tls -- --ignored`: a real HTTPS round trip.
    #[test]
    #[ignore = "needs internet"]
    fn live_tls() {
        if std::env::var_os("SLOT_LIVE_NET").is_none() {
            return;
        }
        let reply = send(&Request {
            tls: true,
            server: "retroachievements.org:443",
            method: "POST",
            path: "/dorequest.php",
            form: Some("r=login2"),
            limit: 1 << 16,
        })
        .unwrap();
        assert_eq!(reply.status, 422, "{}", String::from_utf8_lossy(&reply.body));
    }
}
