//! The time from an internet time server (SNTP, RFC 4330), for System → Auto Clock. Plain UDP:
//! HTTPS can't be used, as it checks certificates against the very clock being set.
use std::net::{ToSocketAddrs, UdpSocket};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Asked in turn until one answers. SLOT_NTP_SERVER names one instead, or none when empty
/// (the simulator's scenarios, whose clock must stay where they put it).
const SERVERS: [&str; 2] = ["pool.ntp.org:123", "time.google.com:123"];

/// Seconds from 1900, NTP's epoch, to 1970.
const NTP_TO_UNIX: u64 = 2_208_988_800;

/// A time before this is a server's mistake, not the date: 2026-01-01.
const EARLIEST: i64 = 1_767_225_600;

pub fn servers() -> Vec<String> {
    match std::env::var("SLOT_NTP_SERVER") {
        Ok(one) if one.is_empty() => Vec::new(),
        Ok(one) => vec![one],
        Err(_) => SERVERS.iter().map(|s| s.to_string()).collect(),
    }
}

/// UTC seconds, from the first of `servers` that answers.
pub fn now(servers: &[String]) -> Result<i64, String> {
    let mut last = "no time server set".to_string();
    for server in servers {
        match ask(server) {
            Ok(secs) => return Ok(secs),
            Err(e) => last = format!("{server}: {e}"),
        }
    }
    Err(last)
}

fn ask(server: &str) -> Result<i64, String> {
    let addr = server
        .to_socket_addrs()
        .map_err(|_| "cannot look it up".to_string())?
        .find(|a| a.is_ipv4())
        .ok_or("no address")?;
    let socket = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
    socket.set_read_timeout(Some(Duration::from_secs(3))).map_err(|e| e.to_string())?;
    // Version 4, client. The transmit time is a value of our own, which the reply must
    // echo back: a stray or forged packet that doesn't is ignored.
    let mut request = [0u8; 48];
    request[0] = 0x23;
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0) ^ 0x5107_c10c;
    request[40..48].copy_from_slice(&nonce.to_be_bytes());
    socket.send_to(&request, addr).map_err(|e| e.to_string())?;
    let mut reply = [0u8; 48];
    loop {
        let (n, from) = socket.recv_from(&mut reply).map_err(|_| "no answer".to_string())?;
        if from == addr && n >= 48 {
            break;
        }
    }
    read(&reply, nonce)
}

/// The server's transmit time from a reply to the request carrying `nonce`.
fn read(reply: &[u8; 48], nonce: u64) -> Result<i64, String> {
    let mode = reply[0] & 0x07;
    let stratum = reply[1];
    if mode != 4 || !(1..16).contains(&stratum) {
        return Err("not a time server's answer".into());
    }
    if reply[24..32] != nonce.to_be_bytes() {
        return Err("an answer to some other request".into());
    }
    let secs = u32::from_be_bytes([reply[40], reply[41], reply[42], reply[43]]) as u64;
    let unix = secs.checked_sub(NTP_TO_UNIX).ok_or("a time before 1970")? as i64;
    if unix < EARLIEST {
        return Err("a time in the past".into());
    }
    Ok(unix)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(nonce: u64, unix: u64) -> [u8; 48] {
        let mut r = [0u8; 48];
        r[0] = 0x24;
        r[1] = 2;
        r[24..32].copy_from_slice(&nonce.to_be_bytes());
        r[40..44].copy_from_slice(&((unix + NTP_TO_UNIX) as u32).to_be_bytes());
        r
    }

    #[test]
    fn a_reply_gives_the_servers_time() {
        assert_eq!(read(&reply(7, 1_800_000_000), 7), Ok(1_800_000_000));
    }

    #[test]
    fn a_reply_to_another_request_or_from_no_server_is_refused() {
        assert!(read(&reply(7, 1_800_000_000), 8).is_err());
        let mut r = reply(7, 1_800_000_000);
        r[1] = 0;
        assert!(read(&r, 7).is_err());
        r = reply(7, 1_800_000_000);
        r[0] = 0x23;
        assert!(read(&r, 7).is_err());
        assert!(read(&reply(7, 1_000_000_000), 7).is_err());
    }

    #[test]
    fn a_server_on_this_machine_is_asked_and_answers() {
        let server = UdpSocket::bind("127.0.0.1:0").unwrap();
        let at = server.local_addr().unwrap().to_string();
        let answer = std::thread::spawn(move || {
            let mut req = [0u8; 48];
            let (_, from) = server.recv_from(&mut req).unwrap();
            assert_eq!(req[0], 0x23);
            let nonce = u64::from_be_bytes(req[40..48].try_into().unwrap());
            server.send_to(&reply(nonce, 1_800_000_123), from).unwrap();
        });
        assert_eq!(now(&[at]), Ok(1_800_000_123));
        answer.join().unwrap();
    }

    #[test]
    fn no_servers_means_no_time() {
        assert!(now(&[]).is_err());
    }
}
