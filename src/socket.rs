#[derive(Debug, Clone, Copy)]
pub enum SocketMode {
    Tcp,
    Udp,
    Unix,
}

impl SocketMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tcp => "tcp",
            Self::Udp => "udp",
            Self::Unix => "unix",
        }
    }
}
