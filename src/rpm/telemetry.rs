//! UDP telemetry input for the shift lights.
//!
//! Binds `0.0.0.0:<port>` (default 26999) and accepts, per datagram:
//! - 2+ raw bytes: first two bytes as little-endian u16 RPM
//! - ASCII: a decimal number (e.g. `7200` or `7200.5`)
//! - JSON: `{"rpm": 7200}`
//!
//! Anything unparseable is ignored (logged at debug). SimHub or a small
//! game-telemetry forwarder can drive this.

use std::net::UdpSocket;
use std::sync::Arc;
use std::time::Duration;

use super::LightsHandle;

/// Spawn the listener thread. Runs until `run` is cleared (checked every
/// 250 ms read timeout); datagrams are ignored while UDP is disabled in
/// the live config.
pub fn spawn_udp_listener(
    handle: Arc<LightsHandle>,
    port: u16,
    run: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> std::io::Result<std::thread::JoinHandle<()>> {
    let sock = UdpSocket::bind(("0.0.0.0", port))?;
    // Never block shutdown: the socket is only polled with a timeout.
    sock.set_read_timeout(Some(Duration::from_millis(250)))?;
    use std::sync::atomic::Ordering;
    std::thread::Builder::new()
        .name("rpm-udp".into())
        .spawn(move || {
            let mut buf = [0u8; 64];
            while run.load(Ordering::Relaxed) {
                match sock.recv_from(&mut buf) {
                    Ok((len, _)) => {
                        if handle.udp_enabled() {
                            if let Some(rpm) = parse_rpm(&buf[..len]) {
                                handle.send_rpm(rpm);
                            }
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut => {}
                    Err(e) => {
                        log::warn!("rpm udp listener error: {}", e);
                        break;
                    }
                }
            }
        })
}

/// Parse one datagram into an RPM value.
pub fn parse_rpm(bytes: &[u8]) -> Option<u16> {
    // Binary datagrams (invalid UTF-8): raw little-endian u16.
    let text = match std::str::from_utf8(bytes) {
        Ok(t) => t.trim(),
        Err(_) => {
            return if bytes.len() >= 2 {
                Some(u16::from_le_bytes([bytes[0], bytes[1]]))
            } else {
                None
            }
        }
    };
    // JSON: look for "rpm": <number>
    if let Some(idx) = text.find("\"rpm\"") {
        let rest = &text[idx + 5..];
        let num: String = rest
            .trim_start()
            .trim_start_matches(':')
            .trim_start()
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        return parse_num(&num);
    }
    // Plain ASCII number
    if !text.is_empty() && text.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '-') {
        return parse_num(text);
    }
    None
}

fn parse_num(s: &str) -> Option<u16> {
    s.parse::<f32>().ok().map(|v| v.clamp(0.0, 65535.0) as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_le_u16() {
        assert_eq!(parse_rpm(&[0xE0, 0x1C]), Some(0x1CE0));
    }

    #[test]
    fn parses_ascii_number() {
        assert_eq!(parse_rpm(b"7200", ), Some(7200));
        assert_eq!(parse_rpm(b" 7200.4 ", ), Some(7200));
    }

    #[test]
    fn parses_json() {
        assert_eq!(parse_rpm(b"{\"speed\":1,\"rpm\":8123}"), Some(8123));
    }

    #[test]
    fn garbage_is_none() {
        assert_eq!(parse_rpm(b"hello"), None);
    }

    #[test]
    fn clamps_out_of_range() {
        assert_eq!(parse_num("999999"), Some(65535));
    }
}
