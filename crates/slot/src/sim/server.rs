//! The simulator over HTTP: a page showing the Brick's screen live, with its buttons on the
//! keyboard, and the API that page (or Playwright, or curl) drives it with.
//!
//! - `GET /`              the page
//! - `GET /frame.png`     the screen now
//! - `GET /state`         phase, frame count, idle time, LEDs, backlight and knobs, as JSON
//! - `POST /press?btn=a&ms=80`, `POST /down?btn=menu`, `POST /up?btn=menu`
//! - `POST /idle?settle=400&timeout=10000`   returns once the screen holds still (408 if not)
//! - `POST /set?knob=battery&value=15`       battery, charging, mute, online
//! - `POST /quit`
//!
//! Meant for this machine only: it answers only requests addressed to localhost, and POSTs
//! must carry `X-Slot-Sim: 1`, which no other web page can send here without asking first.
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::time::Duration;

use slot_input::RawEvent;

use super::fixture::{json_escape, report, set};
use super::{image, parse_btn, phase_name, Shared, PANEL};

const PAGE: &str = include_str!("page.html");
const MAX_HEAD: usize = 8192;

pub fn spawn(addr: &str, shared: Arc<Shared>) {
    let listener = match TcpListener::bind(addr) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("slot: sim: cannot serve on {addr}: {e}");
            std::process::exit(1);
        }
    };
    eprintln!("slot: sim: serving on http://{addr}/");
    let started = std::thread::Builder::new()
        .name("slot-sim-http".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                let shared = shared.clone();
                let _ = std::thread::Builder::new()
                    .name("slot-sim-request".into())
                    .spawn(move || serve(stream, &shared));
            }
        });
    if let Err(e) = started {
        eprintln!("slot: sim http: {e}");
    }
}

struct Request {
    method: String,
    path: String,
    query: Vec<(String, String)>,
    local_host: bool,
    marked: bool,
}

impl Request {
    fn get(&self, key: &str) -> Option<&str> {
        self.query
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

fn read_request(stream: &mut TcpStream) -> Option<Request> {
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    let mut head = Vec::new();
    let mut buf = [0u8; 1024];
    while !head.windows(4).any(|w| w == b"\r\n\r\n") {
        let n = stream.read(&mut buf).ok()?;
        if n == 0 || head.len() + n > MAX_HEAD {
            return None;
        }
        head.extend_from_slice(&buf[..n]);
    }
    let head = String::from_utf8(head).ok()?;
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let method = first.next()?.to_string();
    let target = first.next()?;
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let query = query
        .split('&')
        .filter(|kv| !kv.is_empty())
        .map(|kv| {
            let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
            (k.to_string(), v.to_string())
        })
        .collect();
    let mut local_host = false;
    let mut marked = false;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        if name.eq_ignore_ascii_case("host") {
            let host = value.rsplit_once(':').map_or(value, |(h, port)| {
                if port.chars().all(|c| c.is_ascii_digit()) {
                    h
                } else {
                    value
                }
            });
            local_host = matches!(host, "localhost" | "127.0.0.1" | "[::1]");
        } else if name.eq_ignore_ascii_case("x-slot-sim") {
            marked = value == "1";
        }
    }
    Some(Request {
        method,
        path: path.to_string(),
        query,
        local_host,
        marked,
    })
}

fn respond(stream: &mut TcpStream, status: &str, kind: &str, body: &[u8]) {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
}

fn json(stream: &mut TcpStream, status: &str, body: String) {
    respond(stream, status, "application/json", body.as_bytes());
}

fn error(stream: &mut TcpStream, status: &str, message: &str) {
    json(
        stream,
        status,
        format!("{{\"error\":\"{}\"}}", json_escape(message)),
    );
}

fn state(shared: &Shared) -> String {
    let s = shared.status();
    let idle_ms = s.idle_since.map_or(0, |t| t.elapsed().as_millis());
    format!(
        "{{\"frames\":{},\"phase\":\"{}\",\"phase_name\":\"{}\",\"idle_ms\":{idle_ms},\"sleeping\":{},\"width\":{},\"height\":{},{}}}",
        s.frames,
        json_escape(&s.phase),
        json_escape(phase_name(&s.phase)),
        s.sleeping,
        PANEL.0,
        PANEL.1,
        report()
    )
}

fn millis(req: &Request, key: &str, default: u64, max: u64) -> Duration {
    Duration::from_millis(
        req.get(key)
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(default)
            .min(max),
    )
}

fn serve(mut stream: TcpStream, shared: &Shared) {
    let Some(req) = read_request(&mut stream) else {
        return;
    };
    if !req.local_host {
        return error(&mut stream, "403 Forbidden", "only for localhost");
    }
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/") => respond(
            &mut stream,
            "200 OK",
            "text/html; charset=utf-8",
            PAGE.as_bytes(),
        ),
        ("GET", "/frame.png") => match shared.screenshot(Duration::from_secs(5)) {
            Some(rgba) => respond(
                &mut stream,
                "200 OK",
                "image/png",
                &image::encode(PANEL.0, PANEL.1, &rgba),
            ),
            None => error(&mut stream, "503 Service Unavailable", "no frame"),
        },
        ("GET", "/state") => json(&mut stream, "200 OK", state(shared)),
        ("POST", _) if !req.marked => {
            error(&mut stream, "403 Forbidden", "POSTs need X-Slot-Sim: 1")
        }
        ("POST", "/press" | "/down" | "/up") => {
            let Some(btn) = req.get("btn").and_then(parse_btn) else {
                return error(&mut stream, "400 Bad Request", "btn: up down left right a b x y l1 r1 l2 r2 start select menu volup voldown power");
            };
            match req.path.as_str() {
                "/press" => shared.press(btn, millis(&req, "ms", 80, 10_000).as_millis() as u64),
                "/down" => shared.push(RawEvent::Down(btn)),
                _ => shared.push(RawEvent::Up(btn)),
            }
            json(&mut stream, "200 OK", state(shared))
        }
        ("POST", "/idle") => {
            let settle = millis(&req, "settle", 400, 10_000);
            let timeout = millis(&req, "timeout", 10_000, 60_000);
            if shared.wait_idle(settle, timeout) {
                json(&mut stream, "200 OK", state(shared))
            } else {
                error(
                    &mut stream,
                    "408 Request Timeout",
                    "the screen never settled",
                )
            }
        }
        ("POST", "/set") => match set(
            req.get("knob").unwrap_or(""),
            req.get("value").unwrap_or(""),
        ) {
            Ok(()) => json(&mut stream, "200 OK", state(shared)),
            Err(e) => error(&mut stream, "400 Bad Request", &e),
        },
        ("POST", "/quit") => {
            json(&mut stream, "200 OK", "{}".into());
            shared.quit(0);
        }
        _ => error(&mut stream, "404 Not Found", "no such endpoint"),
    }
}
