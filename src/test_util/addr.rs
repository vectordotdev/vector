//! Test utilities for allocating unique network addresses.
//!
//! This module provides thread-safe port allocation for tests using a guard pattern
//! to prevent port reuse race conditions. The design eliminates intra-process races by:
//! 1. Binding to get a port while holding a TCP listener
//! 2. Registering the port atomically (while still holding the listener and registry lock)
//! 3. Only then releasing the listener (port now protected by registry entry)
//!
//! This ensures no race window between port allocation and registration.

use std::{
    collections::HashSet,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener as StdTcpListener},
    sync::{LazyLock, Mutex},
};

/// Maximum number of attempts to allocate a unique port before panicking.
/// This should be far more than needed since port collisions are rare,
/// but provides a safety net against infinite loops.
const MAX_PORT_ALLOCATION_ATTEMPTS: usize = 100;

/// A guard that reserves a port in the registry, preventing port reuse until dropped.
/// The guard does NOT hold the actual listener - it just marks the port as reserved
/// so that concurrent calls to next_addr() won't return the same port.
pub struct PortGuard {
    addr: SocketAddr,
}

impl PortGuard {
    /// Get the socket address that this guard is holding.
    pub const fn addr(&self) -> SocketAddr {
        self.addr
    }
}

impl Drop for PortGuard {
    fn drop(&mut self) {
        // Remove from the reserved ports set when dropped
        RESERVED_PORTS
            .lock()
            .expect("poisoned lock potentially due to test panicking")
            .remove(&self.addr.port());
    }
}

/// Global set of reserved ports for collision detection. When a test allocates a port, we check this set to ensure the
/// OS didn't recycle a port that's still in use by another test.
/// Ports are tracked by number only (u16). This means IPv4 and IPv6 may block each other from using the same port.
/// This simplification is acceptable for our tests.
static RESERVED_PORTS: LazyLock<Mutex<HashSet<u16>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

/// Allocates a unique port and returns a guard that keeps it reserved.
///
/// The returned `PortGuard` must be kept alive for as long as you need the port reserved.
/// When the guard is dropped, the port is automatically released.
///
/// If the OS assigns a port that's already reserved by another test, this function will
/// automatically retry with a new port, ensuring each test gets a unique port.
///
/// # Example
/// ```ignore
/// let (_guard, addr) = next_addr_for_ip(IpAddr::V4(Ipv4Addr::LOCALHOST));
/// // Use addr for your test
/// // Port is automatically released when _guard goes out of scope
/// ```
pub fn next_addr_for_ip(ip: IpAddr) -> (PortGuard, SocketAddr) {
    for _ in 0..MAX_PORT_ALLOCATION_ATTEMPTS {
        let listener = StdTcpListener::bind((ip, 0)).expect("Failed to bind to OS-assigned port");
        let addr = listener.local_addr().expect("Failed to get local address");
        let port = addr.port();

        // Check if this port is already reserved by another test WHILE still holding the listener
        let mut reserved = RESERVED_PORTS
            .lock()
            .expect("poisoned lock potentially due to test panicking");
        if reserved.contains(&port) {
            // OS recycled a port that's still reserved by another test.
            // Lock and listener will be dropped implicitly after continuing
            continue;
        }

        // Port is unique, mark it as reserved BEFORE dropping the listener
        // This ensures no race window between dropping listener and registering the port
        reserved.insert(port);
        drop(reserved);

        // Now it's safe to drop the listener - the registry protects the port
        drop(listener);

        let guard = PortGuard { addr };
        return (guard, addr);
    }

    panic!("Failed to allocate a unique port after {MAX_PORT_ALLOCATION_ATTEMPTS} attempts");
}

pub fn next_addr() -> (PortGuard, SocketAddr) {
    next_addr_for_ip(IpAddr::V4(Ipv4Addr::LOCALHOST))
}

pub fn next_addr_any() -> (PortGuard, SocketAddr) {
    next_addr_for_ip(IpAddr::V4(Ipv4Addr::UNSPECIFIED))
}

pub fn next_addr_v6() -> (PortGuard, SocketAddr) {
    next_addr_for_ip(IpAddr::V6(Ipv6Addr::LOCALHOST))
}
