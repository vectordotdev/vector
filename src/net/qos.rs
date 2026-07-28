//! DSCP (IP_TOS) QoS helpers: parse names and set/get socket TOS.
use std::io;
#[cfg(any(unix, windows))]
use socket2::SockRef;
use std::net::{TcpStream, UdpSocket};

/// Parse a DSCP name (CS0-CS7, AF11..AF43, EF) into the DSCP 6-bit code.
fn parse_dscp_code(name: &str) -> Option<u8> {
    let s = name.trim();
    if s.is_empty() { return None; }
    // Numeric value (decimal or 0xHEX) as full TOS byte (0-255)
    if let Some(stripped) = s.strip_prefix("0x") { if let Ok(v) = u8::from_str_radix(stripped, 16) { return Some(v >> 2); } }
    if let Ok(v) = s.parse::<u8>() { return Some(v >> 2); }
    let u = s.to_ascii_uppercase();
    if u == "EF" { return Some(46); }
    if let Some(n) = u.strip_prefix("CS") { if let Ok(c) = n.parse::<u8>() { if c <= 7 { return Some(c * 8); } } }
    if let Some(af) = u.strip_prefix("AF") {
        let bytes = af.as_bytes();
        if bytes.len() == 2 {
            let c = bytes[0];
            let d = bytes[1];
            if (b'1'..=b'4').contains(&c) && (b'1'..=b'3').contains(&d) {
                let class = (c - b'0') as u8; // 1..4
                let drop = (d - b'0') as u8;  // 1..3
                // Map per RFC 2597
                let code = match (class, drop) {
                    (1,1)=>10,(1,2)=>12,(1,3)=>14,
                    (2,1)=>18,(2,2)=>20,(2,3)=>22,
                    (3,1)=>26,(3,2)=>28,(3,3)=>30,
                    (4,1)=>34,(4,2)=>36,(4,3)=>38,
                    _=>return None,
                };
                return Some(code);
            }
        }
    }
    None
}

/// Parse user input into a final TOS byte (DSCP<<2), leaving ECN bits zero.
pub fn parse_ip_tos(input: &str) -> Option<u8> {
    let s = input.trim();
    if s.is_empty() { return None; }
    // If numeric, accept as full TOS byte (0-255)
    if let Some(stripped) = s.strip_prefix("0x") { if let Ok(v) = u8::from_str_radix(stripped, 16) { return Some(v); } }
    if let Ok(v) = s.parse::<u8>() { return Some(v); }
    parse_dscp_code(s).map(|code| code << 2)
}

/// Set IP_TOS on a TCP stream if supported; otherwise return Ok(()) and let caller log.
pub fn set_stream_tos(stream: &TcpStream, tos: u8) -> io::Result<()> {
    #[cfg(any(unix, windows))]
    {
        let sref = SockRef::from(stream);
        return sref.set_tos(tos);
    }
    #[allow(unreachable_code)]
    Ok(())
}

/// Set IP_TOS on a UDP socket if supported; otherwise Ok(()) on unsupported platforms.
pub fn set_udp_tos(sock: &UdpSocket, tos: u8) -> io::Result<()> {
    #[cfg(any(unix, windows))]
    {
        let sref = SockRef::from(sock);
        return sref.set_tos(tos);
    }
    #[allow(unreachable_code)]
    Ok(())
}

/// Get current TOS if supported; returns None on unsupported platforms or error.
pub fn get_udp_tos(sock: &UdpSocket) -> Option<u8> {
    #[cfg(any(unix, windows))]
    {
        let sref = SockRef::from(sock);
        return sref.tos().ok().map(|v| v as u8);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_names_to_tos() {
        assert_eq!(parse_ip_tos("CS0"), Some(0));
        assert_eq!(parse_ip_tos("CS3"), Some((24)<<2));
        assert_eq!(parse_ip_tos("EF"), Some(46<<2));
        assert_eq!(parse_ip_tos("AF21"), Some(18<<2));
        assert_eq!(parse_ip_tos("0xB8"), Some(0xB8));
        assert_eq!(parse_ip_tos("184"), Some(184));
    }

    #[test]
    fn parse_invalid() {
        assert_eq!(parse_ip_tos(""), None);
        assert_eq!(parse_ip_tos("AF00"), None);
        assert_eq!(parse_ip_tos("CS9"), None);
    }
}
