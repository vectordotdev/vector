The `socket` source (UDP mode) now supports a `multicast_interface` option that controls which local network interface is used when joining multicast groups. This is useful on hosts with multiple interfaces and on macOS, where specifying `0.0.0.0` only joins on the default interface (unlike Linux, which joins on all interfaces).

authors: thomasqueirozb
