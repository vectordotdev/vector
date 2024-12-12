The `socket` sink now supports the `unix_mode` configuration option that specifies the Unix socket mode to use. Valid values:

- `Stream` (default) - Stream-oriented (`SOCK_STREAM`)
- `Datagram` - Datagram-oriented (`SOCK_DGRAM`)

This option only applies when `mode = "unix"`, and is unavailable on macOS, where `SOCK_STREAM` is always used for Unix sockets.

authors: jpovixwm
