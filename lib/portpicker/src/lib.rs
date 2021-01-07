// Modified by `Vector Contributors <vector@timber.io>`.
// Based on `https://github.com/Dentosal/portpicker-rs` by `Hannes Karppila <hannes.karppila@gmail.com>`.
// `portpicker-rs` LICENSE:
// This is free and unencumbered software released into the public domain.

// Anyone is free to copy, modify, publish, use, compile, sell, or
// distribute this software, either in source code form or as a compiled
// binary, for any purpose, commercial or non-commercial, and by any
// means.

// In jurisdictions that recognize copyright laws, the author or authors
// of this software dedicate any and all copyright interest in the
// software to the public domain. We make this dedication for the benefit
// of the public at large and to the detriment of our heirs and
// successors. We intend this dedication to be an overt act of
// relinquishment in perpetuity of all present and future rights to this
// software under copyright law.

// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR ANY CLAIM, DAMAGES OR
// OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE,
// ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
// OTHER DEALINGS IN THE SOFTWARE.

// For more information, please refer to <http://unlicense.org>

use rand::{thread_rng, Rng};
use std::net::{
    Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6, TcpListener, ToSocketAddrs, UdpSocket,
};

pub type Port = u16;

// Try to bind to a socket using UDP
fn test_bind_udp<A: ToSocketAddrs>(addr: A) -> Option<Port> {
    Some(UdpSocket::bind(addr).ok()?.local_addr().ok()?.port())
}

// Try to bind to a socket using TCP
fn test_bind_tcp<A: ToSocketAddrs>(addr: A) -> Option<Port> {
    Some(TcpListener::bind(addr).ok()?.local_addr().ok()?.port())
}

/// Check if a port is free on UDP
pub fn is_free_udp(port: Port) -> bool {
    let ipv4 = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
    let ipv6 = SocketAddrV6::new(Ipv6Addr::LOCALHOST, port, 0, 0);

    test_bind_udp(ipv6).is_some() && test_bind_udp(ipv4).is_some()
}

/// Check if a port is free on TCP
pub fn is_free_tcp(port: Port) -> bool {
    let ipv4 = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
    let ipv6 = SocketAddrV6::new(Ipv6Addr::LOCALHOST, port, 0, 0);

    test_bind_tcp(ipv6).is_some() && test_bind_tcp(ipv4).is_some()
}

/// Check if a port is free on both TCP and UDP
pub fn is_free(port: Port) -> bool {
    is_free_tcp(port) && is_free_udp(port)
}

/// Asks the OS for a free port
fn ask_free_tcp_port() -> Option<Port> {
    let ipv4 = SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0);
    let ipv6 = SocketAddrV6::new(Ipv6Addr::LOCALHOST, 0, 0, 0);

    test_bind_tcp(ipv6).or_else(|| test_bind_tcp(ipv4))
}

/// Picks an available port that is available on both TCP and UDP
/// ```rust
/// use portpicker::pick_unused_port;
/// let port: u16 = pick_unused_port();
/// ```
pub fn pick_unused_port() -> Port {
    let mut rng = thread_rng();

    loop {
        // Try random port first
        for _ in 0..10 {
            let port = rng.gen_range(15000..25000);
            if is_free(port) {
                return port;
            }
        }

        // Ask the OS for a port
        for _ in 0..10 {
            if let Some(port) = ask_free_tcp_port() {
                // Test that the udp port is free as well
                if is_free_udp(port) {
                    return port;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::pick_unused_port;

    #[test]
    fn it_works() {
        pick_unused_port();
    }
}
