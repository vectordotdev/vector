Fixed TLS connections to a bracketed IPv6 address, such as `https://[::1]:8080`, in HTTP-based and socket sinks. Vector no longer sends the bracketed address as SNI and now verifies the certificate against the IP address.

authors: anpawo
